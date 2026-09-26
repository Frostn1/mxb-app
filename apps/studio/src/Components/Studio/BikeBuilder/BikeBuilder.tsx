import { useCallback, useMemo, useState } from "react";
import { Box } from "lucide-react";
import { toast } from "sonner";
import { useT } from "@/i18n";
import { ContextBarRight } from "../../Shell/ContextBar";
import { addPart, addPlaceholderBike, setPartRole, setSlot, setTemplate } from "../../../api/bikebuild";
import BaseBikeStep from "./BaseBikeStep";
import BlenderBar from "./BlenderBar";
import PartTray from "./PartTray";
import PartSlots from "./PartSlots";
import PreviewPane, { useAssemblyView } from "./PreviewPane";
import BuildPanel from "./BuildPanel";
import StepHeader, { type BuildStep } from "./StepHeader";
import { useBikeLibrary } from "./useBikeLibrary";
import { useBlenderStatus } from "./useBlenderStatus";
import { usePartDrag } from "./usePartDrag";

/**
 * Bike builder: put a bike together from parts, without having to be good at Blender.
 *
 * Studio chooses the parts and where they go; the rider's own Blender, run in the
 * background, does the importing, placing and exporting (see `src-tauri/src/blender.rs`).
 * One screen, one header row: the step breadcrumb and the base-bike picker share it — Blender's
 * status moved out entirely, into the title bar's own right-hand slot, since it's a fact about
 * the tool, not a step in building a bike. Below that, the 3D view is the main panel — placing
 * a part means dragging one from the tray onto it, clicking an open mount, or the tray's own
 * "Place ▸" — the tray sits to its left, and the role outliner is a collapsible strip on the
 * right, read-only, for checking what's still missing. Build is a small floating control in
 * the bottom-right corner, not a footer of its own. Part Maker joins the tray as a dialog.
 */
export default function BikeBuilder() {
  const t = useT();
  const blender = useBlenderStatus();
  /** Bumped whenever one panel changes the library, so the others read it again. */
  const [version, setVersion] = useState(0);
  const changed = useCallback(() => setVersion((v) => v + 1), []);
  const [view, setView] = useAssemblyView(version);
  const [baseBusy, setBaseBusy] = useState(false);
  const lib = useBikeLibrary(version, changed, baseBusy);
  /** The backend always has *some* template loaded — a fresh build defaults to the
   *  placeholder without anyone having asked for it — so `view.template` alone can't say
   *  whether the rider has actually done step 1 yet. Set the moment one of this step's own
   *  picks lands, so the step indicator and its hint don't treat "never touched" the same as
   *  "chose the placeholder". A saved build that already has a chassis part or a real bike
   *  template counts too, for a build reopened from a previous session. */
  const [explicitBase, setExplicitBase] = useState(false);
  const { dragging, startDrag } = usePartDrag((part) => {
    if (part.role) void lib.onSlot(part.role, part.id);
  });

  async function onPickTemplate(path: string) {
    setBaseBusy(true);
    try {
      setView(await setTemplate(path));
      setExplicitBase(true);
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
      setExplicitBase(true);
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
      setExplicitBase(true);
      changed();
    } catch (e) {
      toast.error(t("bike.placeholderFailed"), { description: String(e) });
    } finally {
      setBaseBusy(false);
    }
  }

  // The whole-bike part playing chassis, when there is one — the "Split into parts" action in
  // step 1 cuts *this* up, not the template (the template is just the anchor geometry parts
  // snap to; splitting it wouldn't mean anything).
  const chassisPart = useMemo(
    () => lib.parts?.find((p) => p.role === "chassis" && lib.slots.chassis === p.id) ?? null,
    [lib.parts, lib.slots.chassis],
  );
  // What the base-bike button shows: the chassis part's own name when there is one — that's
  // what a rider thinks of as "the base bike" after importing a file or using one as the base
  // — falling back to the template's name only when nothing's been brought in as chassis yet
  // (an installed bike picked for its anchors alone, or truly nothing). Showing the template's
  // name here regardless used to read as "Studio's placeholder" right after importing a real
  // file as the base, since the anchor template stays the placeholder either way.
  const baseName = chassisPart
    ? chassisPart.name
    : view
      ? view.template.source.kind === "placeholder"
        ? t("bike.placeholderTemplate")
        : view.template.name
      : null;
  const baseChosen = explicitBase || !!chassisPart || view?.template.source.kind === "bike";
  const placedCount = view?.assembly.placed.length ?? 0;
  const step: BuildStep = !baseChosen ? 1 : placedCount <= 1 ? 2 : 3;

  return (
    <div className="relative flex h-full min-h-0 flex-col">
      <ContextBarRight>
        <BlenderBar blender={blender} />
      </ContextBarRight>

      <div className="flex flex-wrap items-center justify-between gap-3 border-b border-border bg-window px-4 py-2">
        <StepHeader step={step} />
        <BaseBikeStep
          baseName={baseName}
          baseChosen={baseChosen}
          problem={view?.template.problem ?? null}
          onPick={onPickTemplate}
          onImportFile={onImportFile}
          onPlaceholder={onPlaceholder}
          busy={baseBusy}
          blenderReady={blender.ready}
          onSplitBase={chassisPart ? () => void lib.onSplit(chassisPart) : undefined}
          splitBusy={lib.busy}
        />
      </div>

      {dragging && (
        <div
          className="pointer-events-none fixed z-50 flex items-center gap-1.5 border border-primary bg-popover px-2 py-1 text-[12px] shadow-lg"
          style={{ left: dragging.x + 12, top: dragging.y + 12 }}
        >
          <Box className="size-3.5 text-primary" />
          {dragging.part.name}
        </div>
      )}

      <div className="relative min-h-0 flex-1">
        <div className="grid h-full min-h-0 grid-cols-[18rem_1fr_auto] divide-x divide-border overflow-hidden">
          <PartTray ready={blender.ready} lib={lib} onStartDrag={startDrag} onChanged={changed} />
          <PreviewPane version={version} view={view} setView={setView} lib={lib} baseChosen={baseChosen} />
          <PartSlots lib={lib} />
        </div>
        <BuildPanel view={view} placedCount={placedCount} />
      </div>
    </div>
  );
}
