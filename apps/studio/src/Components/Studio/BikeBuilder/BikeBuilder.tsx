import { useCallback, useState } from "react";
import { Box } from "lucide-react";
import { toast } from "sonner";
import { useT } from "@/i18n";
import { addPart, addPlaceholderBike, setPartRole, setSlot, setTemplate } from "../../../api/bikebuild";
import BaseBikeStep from "./BaseBikeStep";
import BlenderBar from "./BlenderBar";
import PartTray from "./PartTray";
import PartSlots from "./PartSlots";
import PreviewPane, { useAssemblyView } from "./PreviewPane";
import BuildPanel from "./BuildPanel";
import { useBikeLibrary } from "./useBikeLibrary";
import { usePartDrag } from "./usePartDrag";

/**
 * Bike builder: put a bike together from parts, without having to be good at Blender.
 *
 * Studio chooses the parts and where they go; the rider's own Blender, run in the
 * background, does the importing, placing and exporting (see `src-tauri/src/blender.rs`).
 * One screen: `BaseBikeStep` picks what the build starts from (an installed bike, a full-bike
 * file, or the placeholder), the 3D view is the main panel — placing a part means dragging one
 * from the tray onto it, clicking an open mount, or the tray's own "Place ▸" — the tray sits to
 * its left, and the role outliner is a collapsible strip on the right, read-only, for checking
 * what's still missing. Part Maker joins the tray as a dialog, not a separate mode.
 */
export default function BikeBuilder() {
  const t = useT();
  const [ready, setReady] = useState(false);
  /** Bumped whenever one panel changes the library, so the others read it again. */
  const [version, setVersion] = useState(0);
  const changed = useCallback(() => setVersion((v) => v + 1), []);
  const [view, setView] = useAssemblyView(version);
  const [baseBusy, setBaseBusy] = useState(false);
  const lib = useBikeLibrary(version, changed, baseBusy);
  const { dragging, startDrag } = usePartDrag((part) => {
    if (part.role) void lib.onSlot(part.role, part.id);
  });

  async function onPickTemplate(path: string) {
    setBaseBusy(true);
    try {
      setView(await setTemplate(path));
    } catch (e) {
      toast.error(t("bike.templateFailed"), { description: String(e) });
    } finally {
      setBaseBusy(false);
    }
  }

  /** Brings a file straight into the tray and onto the bike as its base, in one step — the
   *  rider already said what it is by picking this over "Add parts…". Done with the plain API
   *  rather than through `lib`'s own `change()`, which always resolves (it reports failure by
   *  toast and swallows the error) — this needs to actually know it failed, so it doesn't
   *  reset the template out from under a base that was never successfully imported. */
  async function onImportFile(file: string) {
    setBaseBusy(true);
    try {
      const part = await addPart(file);
      await setPartRole(part.id, "chassis");
      await setSlot("chassis", part.id);
      setView(await setTemplate(null));
      changed();
    } catch (e) {
      toast.error(t("bike.importBaseFailed"), { description: String(e) });
    } finally {
      setBaseBusy(false);
    }
  }

  async function onPlaceholder() {
    setBaseBusy(true);
    try {
      await addPlaceholderBike();
      setView(await setTemplate(null));
      changed();
    } catch (e) {
      toast.error(t("bike.placeholderFailed"), { description: String(e) });
    } finally {
      setBaseBusy(false);
    }
  }

  const baseName = view
    ? view.template.source.kind === "placeholder"
      ? t("bike.placeholderTemplate")
      : view.template.name
    : null;

  return (
    <div className="relative flex h-full min-h-0 flex-col">
      <BlenderBar onReadyChange={setReady} />
      {dragging && (
        <div
          className="pointer-events-none fixed z-50 flex items-center gap-1.5 border border-primary bg-popover px-2 py-1 text-[12px] shadow-lg"
          style={{ left: dragging.x + 12, top: dragging.y + 12 }}
        >
          <Box className="size-3.5 text-primary" />
          {dragging.part.name}
        </div>
      )}

      <BaseBikeStep
        baseName={baseName}
        problem={view?.template.problem ?? null}
        onPick={onPickTemplate}
        onImportFile={onImportFile}
        onPlaceholder={onPlaceholder}
        busy={baseBusy}
        blenderReady={ready}
      />

      <div className="flex min-h-0 flex-1 flex-col">
        <div className="grid min-h-0 flex-1 grid-cols-[18rem_1fr_auto] divide-x divide-border overflow-hidden">
          <PartTray ready={ready} lib={lib} onStartDrag={startDrag} onChanged={changed} />
          <PreviewPane version={version} view={view} setView={setView} lib={lib} />
          <PartSlots lib={lib} />
        </div>
        <BuildPanel view={view} placedCount={view?.assembly.placed.length ?? 0} />
      </div>
    </div>
  );
}
