import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { ArrowRightToLine, Box, Loader2, Plus, Scissors, Sparkles } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@frost/shared/Components/ui/button";
import { Dialog, DialogContent, DialogHeader, DialogTitle } from "@frost/shared/Components/ui/dialog";
import { Popover, PopoverAnchor, PopoverContent } from "@frost/shared/Components/ui/popover";
import { TooltipProvider } from "@frost/shared/Components/ui/tooltip";
import { cn } from "@frost/shared/lib/utils";
import { useT } from "@/i18n";
import { flag } from "../../../lib/flags";
import {
  ROLES,
  addPart,
  addPlaceholderBike,
  nudge,
  rotate,
  setPartRole,
  setSlot,
  setTemplate,
  type AssemblyView,
  type LibraryPart,
  type Role,
  type V3,
} from "../../../api/bikebuild";
import BaseBikeStep from "./BaseBikeStep";
import BlenderNeeded, { useBlenderGate, type BlenderAction } from "./BlenderNeeded";
import BuildPanel from "./BuildPanel";
import Outliner, { type PartActions, type Selection } from "./Outliner";
import PartMaker from "./PartMaker";
import PreviewPane, { useAssemblyView } from "./PreviewPane";
import Properties from "./Properties";
import StepHeader, { type BuildStep } from "./StepHeader";
import { nudgeSteps, type BikeCommand } from "./bikeKeys";
import { nothingToSplit } from "./splitErrors";
import { Tip } from "./Tip";
import { useBikeHistory } from "./useBikeHistory";
import { useBikeLibrary } from "./useBikeLibrary";
import { useBlenderStatus } from "./useBlenderStatus";
import { usePartDrag } from "./usePartDrag";

/** Put a role's nudge at `target` (metres, Blender's frame), whatever it is now: back to
 *  where it snapped, then out in steps `bike_nudge` takes. Null puts it back. */
async function nudgeTo(role: Role, target: V3 | null): Promise<AssemblyView> {
  let view = await nudge(role, null);
  if (target && target.some((v) => Math.abs(v) > 1e-7)) {
    for (const step of nudgeSteps(target)) view = await nudge(role, step);
  }
  return view;
}

/**
 * Bike builder: put a bike together from parts, without having to be good at Blender.
 *
 * Laid out like Blender, since that's the tool riders who build bikes already know: one thin
 * action bar on top (the stages, Add part / Split / Place, and the base bike), the outliner
 * on the left (the bike → its slots → the part in each), the 3D view in the middle and nothing else there, Properties for the selection on the right, and
 * the build bar along the bottom. The outliner and Properties scroll on their own and the 3D
 * view gives way first, so a short window still shows the build bar in full.
 *
 * Studio chooses the parts and where they go; the rider's own Blender, run in the background,
 * does the importing, placing and exporting (see `src-tauri/src/blender.rs`). Blender is only
 * mentioned when an action that needs it can't find it — see `BlenderNeeded`.
 */
export default function BikeBuilder() {
  const t = useT();
  const blender = useBlenderStatus();
  const gate = useBlenderGate(blender);
  /** Bumped whenever one panel changes the library, so the others read it again. */
  const [version, setVersion] = useState(0);
  const changed = useCallback(() => setVersion((v) => v + 1), []);
  const [view, setView] = useAssemblyView(version);
  const [baseBusy, setBaseBusy] = useState(false);
  const lib = useBikeLibrary(version, changed, baseBusy);
  /** The backend always has *some* template loaded — a fresh build defaults to the
   *  placeholder without anyone having asked for it — so `view.template` alone can't say
   *  whether the rider has actually done step 1 yet. Set the moment one of this step's own
   *  picks lands. A saved build that already has a chassis part or a real bike template
   *  counts too, for a build reopened from a previous session. */
  const [explicitBase, setExplicitBase] = useState(false);
  const [selection, setSelection] = useState<Selection>(null);
  const [hidden, setHidden] = useState<Set<string>>(() => new Set());
  const [pickerOpen, setPickerOpen] = useState(false);
  const [making, setMaking] = useState(false);
  /** N: the Properties panel shown (as Blender's sidebar). */
  const [propsOpen, setPropsOpen] = useState(true);
  const [helpOpen, setHelpOpen] = useState(false);
  /** X pressed on this part: asking first. */
  const [takingOff, setTakingOff] = useState<LibraryPart | null>(null);
  /** A short note over the 3D view, for a key that had nothing to act on. */
  const [flash, setFlash] = useState<string | null>(null);
  const flashTimer = useRef(0);
  const say = useCallback((text: string) => {
    setFlash(text);
    window.clearTimeout(flashTimer.current);
    flashTimer.current = window.setTimeout(() => setFlash(null), 2600);
  }, []);
  useEffect(() => () => window.clearTimeout(flashTimer.current), []);
  const history = useBikeHistory();
  const nameRef = useRef<HTMLInputElement>(null);
  const { dragging, startDrag } = usePartDrag((part, slot) => {
    if (slot && slot !== part.role) void moveIntoSlot(part, slot);
    else if (part.role) void fillSlot(part.role, part.id);
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

  /** Brings a file straight onto the bike as its base, in one step. Done with the plain API
   *  rather than through `lib`'s own `change()`, which always resolves (it reports failure by
   *  toast and swallows the error) — this needs to actually know it failed, so it doesn't
   *  reset the template out from under a base that was never successfully imported. */
  async function onImportFile(file: string) {
    setBaseBusy(true);
    try {
      // Whole, not split by node: the rider picked this file as the bike itself.
      const [part] = await addPart(file, { split: false });
      await setPartRole(part.id, "chassis");
      await setSlot("chassis", part.id);
      setView(await setTemplate(null));
      setExplicitBase(true);
      setSelection({ kind: "part", id: part.id });
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

  const byId = useMemo(() => new Map((lib.parts ?? []).map((p) => [p.id, p])), [lib.parts]);
  const chassisPart = lib.slots.chassis ? (byId.get(lib.slots.chassis) ?? null) : null;
  // What the base-bike button shows: the chassis part's own name when there is one — that's
  // what a rider thinks of as "the base bike" after importing a file — falling back to the
  // template's name only when nothing's been brought in as chassis yet.
  const baseName = chassisPart
    ? chassisPart.name
    : view
      ? view.template.source.kind === "placeholder"
        ? t("bike.placeholderTemplate")
        : view.template.name
      : null;
  const baseChosen = explicitBase || !!chassisPart || view?.template.source.kind === "bike";
  const placedCount = view?.assembly.placed.length ?? 0;
  const slotsFilled = ROLES.filter((r) => lib.slots[r] && byId.has(lib.slots[r]!)).length;

  // How far the build has got, and which stage is being looked at: the stage line's finished
  // stages are buttons back, and stepping back is only ever a look — nothing is undone until
  // a choice made there says so (and asks first).
  const reached: BuildStep = !baseChosen ? 1 : placedCount <= 1 ? 2 : 3;
  const [stepBack, setStepBack] = useState<BuildStep | null>(null);
  useEffect(() => setStepBack(null), [reached]);
  const active: BuildStep = pickerOpen ? 1 : (stepBack ?? reached);
  function onGoStep(step: BuildStep) {
    if (step === 1) {
      setPickerOpen(true);
      return;
    }
    setPickerOpen(false);
    setStepBack(step === reached ? null : step);
    if (step === 2) {
      // Parts: point at the first slot still to fill, or the part already selected.
      const empty = ROLES.find((r) => !lib.slots[r] || !byId.has(lib.slots[r]!));
      if (empty && selection === null) setSelection({ kind: "slot", role: empty });
    } else {
      nameRef.current?.focus();
    }
  }

  // A selected part that's gone (removed, or replaced by a split) drops the selection.
  useEffect(() => {
    if (selection?.kind === "part" && lib.parts && !byId.has(selection.id)) setSelection(null);
  }, [selection, lib.parts, byId]);

  const selectedPart: LibraryPart | null =
    selection?.kind === "part" ? (byId.get(selection.id) ?? null) : null;
  const isPlaced = (p: LibraryPart) => !!p.role && lib.slots[p.role] === p.id;
  const selectedRole: Role | null = selectedPart && isPlaced(selectedPart) ? selectedPart.role : null;
  const placedRoles = useMemo(() => (view?.assembly.placed ?? []).map((p) => p.role), [view]);
  // A: every part on the bike. Gone again once there's nothing on it.
  useEffect(() => {
    if (selection?.kind === "all" && placedRoles.length === 0) setSelection(null);
  }, [selection, placedRoles]);
  const selectedRoles: Role[] = selection?.kind === "all" ? placedRoles : selectedRole ? [selectedRole] : [];
  const partName = (role: Role) => {
    const id = lib.slots[role];
    return (id && byId.get(id)?.name) || t(`bike.role.${role}`);
  };

  // ---- Edits that undo: a slot filled or emptied, a part moved or turned.

  /** Fill (or, with null, empty) a slot, keeping what was there for undo. */
  async function fillSlot(role: Role, id: string | null) {
    const before = lib.slots[role] ?? null;
    if (before === id) return;
    const name = id ? (byId.get(id)?.name ?? t(`bike.role.${role}`)) : partName(role);
    try {
      await setSlot(role, id);
    } catch (e) {
      toast.error(t("bike.slotFailed"), { description: String(e) });
      return;
    } finally {
      changed();
    }
    const run = (to: string | null) => async () => {
      await setSlot(role, to);
      changed();
    };
    history.push({
      label: t(id ? "bike.history.place" : "bike.history.takeOff", { part: name }),
      undo: run(before),
      redo: run(id),
    });
  }

  /** Dropped on another slot than its role's: it takes that role, then the slot. */
  async function moveIntoSlot(part: LibraryPart, role: Role) {
    try {
      await setPartRole(part.id, role);
    } catch (e) {
      toast.error(t("bike.roleFailed"), { description: String(e) });
      changed();
      return;
    }
    await fillSlot(role, part.id);
  }

  const placedOf = (role: Role) => view?.assembly.placed.find((p) => p.role === role) ?? null;

  /** Where a part's nudge should end up (metres); null puts it back where it snapped. */
  async function setNudge(role: Role, target: V3 | null) {
    const before = placedOf(role)?.nudge ?? null;
    try {
      setView(await nudgeTo(role, target));
    } catch (e) {
      toast.error(t("bike.nudgeFailed"), { description: String(e) });
      return;
    }
    history.push({
      label: t("bike.history.move", { part: partName(role) }),
      undo: async () => setView(await nudgeTo(role, before)),
      redo: async () => setView(await nudgeTo(role, target)),
    });
  }

  /** Where a part's turn should end up (XYZ degrees); null puts it back as it snapped. */
  async function setRotation(role: Role, target: V3 | null) {
    const before = (placedOf(role)?.rotation ?? null) as V3 | null;
    try {
      setView(await rotate(role, target));
    } catch (e) {
      toast.error(t("bike.rotateFailed"), { description: String(e) });
      return;
    }
    history.push({
      label: t("bike.history.turn", { part: partName(role) }),
      undo: async () => setView(await rotate(role, before)),
      redo: async () => setView(await rotate(role, target)),
    });
  }

  function onGrab(role: Role, delta: V3) {
    const now = placedOf(role)?.nudge ?? [0, 0, 0];
    void setNudge(role, now.map((v, i) => Math.round((v + delta[i]) * 10_000) / 10_000) as V3);
  }

  async function undoRedo(which: "undo" | "redo") {
    try {
      const label = which === "undo" ? await history.undo() : await history.redo();
      if (label) toast(t(which === "undo" ? "bike.undone" : "bike.redone", { what: label }), { duration: 1800 });
      else if (!(which === "undo" ? history.canUndo : history.canRedo))
        say(t(which === "undo" ? "bike.nothingToUndo" : "bike.nothingToRedo"));
    } catch (e) {
      toast.error(t("bike.undoFailed"), { description: String(e) });
    }
  }

  const actions: PartActions = {
    place: (p) => {
      if (p.role) void fillSlot(p.role, p.id);
    },
    takeOff: (p) => {
      if (p.role && lib.slots[p.role] === p.id) void fillSlot(p.role, null);
    },
    split: (p) => {
      if (gate.need("split")) void lib.onSplit(p);
    },
    refresh: (p) => {
      if (gate.need("refresh")) void lib.add([p.source]);
    },
    useAsBase: (p) => {
      setExplicitBase(true);
      void lib.onUseAsBase(p);
    },
    remove: (p) => void lib.onRemove(p),
  };

  function onAdd() {
    if (gate.need("add")) void lib.onAdd();
  }

  /** The Bike keys other than G and R (those run in the 3D view itself). */
  function onCommand(cmd: BikeCommand) {
    const target = selectedPart && isPlaced(selectedPart) ? selectedPart : null;
    switch (cmd.kind) {
      case "hide":
        if (selection?.kind === "all") {
          const ids = placedRoles.map((r) => lib.slots[r]).filter((id): id is string => !!id);
          setHidden(new Set(ids));
        } else if (target) setHidden((h) => new Set(h).add(target.id));
        else say(t("bike.keys.selectFirst"));
        break;
      case "unhideAll":
        setHidden(new Set());
        break;
      case "takeOff":
        if (target && !lib.busy) setTakingOff(target);
        else if (!target) say(t("bike.keys.selectFirst"));
        break;
      case "selectAll":
        if (placedRoles.length) setSelection({ kind: "all" });
        break;
      case "deselectAll":
        setSelection(null);
        break;
      case "toggleProperties":
        setPropsOpen((o) => !o);
        break;
      case "undo":
      case "redo":
        void undoRedo(cmd.kind);
        break;
      case "add":
        onAdd();
        break;
      case "split":
        if (selectedPart && !nothingToSplit(selectedPart) && !lib.busy) actions.split(selectedPart);
        else if (!selectedPart) say(t("bike.keys.selectFirst"));
        else if (nothingToSplit(selectedPart)) say(t("bike.splitNothing"));
        break;
      case "place":
        if (selectedPart?.role && !isPlaced(selectedPart) && !lib.busy) actions.place(selectedPart);
        else if (!selectedPart) say(t("bike.keys.selectFirst"));
        else if (!selectedPart.role) say(t("bike.placeNeedsRole"));
        break;
      case "help":
        setHelpOpen((o) => !o);
        break;
    }
  }

  // Undo and redo answer anywhere on the tab, not only over the 3D view: a field edited in
  // Properties is taken back from where it was made. Never while a text field has the keys
  // (its own undo is the browser's), and only while this tab is the one showing.
  const root = useRef<HTMLDivElement>(null);
  const undoKeys = useRef(undoRedo);
  undoKeys.current = undoRedo;
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const el = root.current;
      if (!el || !el.offsetParent || e.altKey || !(e.ctrlKey || e.metaKey)) return;
      if (isTyping(e.target) || isTyping(document.activeElement)) return;
      // Over the 3D view the view itself has these (see `Preview3D`).
      if ((e.target as Element | null)?.closest?.("[data-bike-viewport]")) return;
      if (!el.matches(":hover") && !el.contains(document.activeElement)) return;
      const redo = (e.code === "KeyZ" && e.shiftKey) || (e.code === "KeyY" && !e.shiftKey);
      const undo = e.code === "KeyZ" && !e.shiftKey;
      if (!undo && !redo) return;
      e.preventDefault();
      void undoKeys.current(undo ? "undo" : "redo");
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const showMaker = flag("partMaker");

  return (
    <TooltipProvider delayDuration={350}>
    <div ref={root} className="relative flex h-full min-h-0 flex-col">
      {/* The tab's one action bar. Its own row rather than portalled into the shell's
          context strip: that strip is hidden whenever another tool asks the shell to go bare
          (the Designer's start screen does, and stays mounted), and the stages and Add part
          must never disappear with it. */}
      <div className="flex h-9 shrink-0 items-center gap-2 border-b border-border bg-window px-2">
        <StepHeader reached={reached} active={active} onGo={onGoStep} />
        <span className="mx-1 h-4 w-px shrink-0 bg-border" />
        <div className="flex min-w-0 flex-1 items-center gap-0.5">
          <Gated gate={gate} at="add">
            <Tip label={t("bike.addPartTip")} keys="Shift+A">
              <Button size="sm" variant="ghost" className="h-7 px-2.5" onClick={onAdd} disabled={lib.busy}>
                {lib.adding ? <Loader2 className="size-3.5 animate-spin" /> : <Plus className="size-3.5" />}
                {lib.adding ? t("bike.adding", { n: lib.adding.n, of: lib.adding.of }) : t("bike.addPart")}
              </Button>
            </Tip>
          </Gated>
          <Gated gate={gate} at={["split", "refresh"]}>
            <Tip
              label={
                !selectedPart
                  ? t("bike.selectPartFirst")
                  : nothingToSplit(selectedPart)
                    ? t("bike.splitNothing")
                    : t("bike.splitIntoParts")
              }
              keys="P"
            >
              <Button
                size="sm"
                variant="ghost"
                className="h-7 px-2.5"
                onClick={() => selectedPart && actions.split(selectedPart)}
                disabled={lib.busy || !selectedPart || nothingToSplit(selectedPart)}
              >
                {lib.splitting ? <Loader2 className="size-3.5 animate-spin" /> : <Scissors className="size-3.5" />}
                {t("bike.split")}
              </Button>
            </Tip>
          </Gated>
          <Tip
            label={
              !selectedPart
                ? t("bike.selectPartFirst")
                : !selectedPart.role
                  ? t("bike.placeNeedsRole")
                  : isPlaced(selectedPart)
                    ? t("bike.alreadyPlaced")
                    : t("bike.placeTip")
            }
            keys="Ctrl+P"
          >
            <Button
              size="sm"
              variant="ghost"
              className="h-7 px-2.5"
              onClick={() => selectedPart && actions.place(selectedPart)}
              disabled={lib.busy || !selectedPart || !selectedPart.role || isPlaced(selectedPart)}
            >
              <ArrowRightToLine className="size-3.5" />
              {t("bike.place")}
            </Button>
          </Tip>
          {showMaker && (
            <Gated gate={gate} at="make">
              <Tip label={t("bike.makerHint")}>
                <Button
                  size="sm"
                  variant="ghost"
                  className="h-7 px-2.5"
                  onClick={() => gate.need("make") && setMaking(true)}
                  disabled={lib.busy}
                >
                  <Sparkles className="size-3.5" />
                  {t("bike.makeAPart")}
                </Button>
              </Tip>
            </Gated>
          )}
        </div>
        <BaseBikeStep
          open={pickerOpen}
          onOpenChange={setPickerOpen}
          baseName={baseName}
          baseChosen={baseChosen}
          problem={view?.template.problem ?? null}
          onPick={onPickTemplate}
          onImportFile={onImportFile}
          onPlaceholder={onPlaceholder}
          busy={baseBusy || lib.busy}
          gate={gate}
          slotsFilled={slotsFilled}
          chassisFilled={!!chassisPart}
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

      <div
        className={cn(
          "grid min-h-0 flex-1 divide-x divide-border overflow-hidden",
          propsOpen
            ? "grid-cols-[13rem_minmax(0,1fr)_14rem] xl:grid-cols-[16rem_minmax(0,1fr)_17rem]"
            : "grid-cols-[13rem_minmax(0,1fr)] xl:grid-cols-[16rem_minmax(0,1fr)]",
        )}
      >
        <Outliner
          lib={lib}
          baseName={baseChosen ? baseName : null}
          selection={selection}
          onSelect={setSelection}
          hidden={hidden}
          onToggleHidden={(id) =>
            setHidden((h) => {
              const next = new Set(h);
              if (next.has(id)) next.delete(id);
              else next.add(id);
              return next;
            })
          }
          actions={actions}
          onStartDrag={startDrag}
        />
        <PreviewPane
          version={version}
          view={view}
          lib={lib}
          baseChosen={baseChosen}
          reached={reached}
          selectedRoles={selectedRoles}
          transformRole={selection?.kind === "all" ? (lib.slots.chassis ? "chassis" : null) : selectedRole}
          onSelectRole={(role) => {
            const id = role ? lib.slots[role] : null;
            setSelection(id ? { kind: "part", id } : null);
          }}
          hidden={hidden}
          onCommand={onCommand}
          onGrab={onGrab}
          onRotate={(role, rotation) => {
            if (selection?.kind === "all") say(t("bike.keys.cantTurnAll"));
            else void setRotation(role, rotation);
          }}
          onNothingToTransform={() => say(t("bike.keys.selectFirst"))}
          onPickBase={() => setPickerOpen(true)}
          takingOff={takingOff}
          onTakeOff={(p) => {
            setTakingOff(null);
            actions.takeOff(p);
          }}
          onCancelTakeOff={() => setTakingOff(null)}
          helpOpen={helpOpen}
          onHelpOpen={setHelpOpen}
          flash={flash}
          onFill={(role, id) => void fillSlot(role, id)}
        />
        {propsOpen && (
          <Properties
            selection={selection}
            lib={lib}
            view={view}
            edits={{
              setNudge: (role, v) => void setNudge(role, v),
              setRotation: (role, v) => void setRotation(role, v),
            }}
            actions={actions}
            onAdd={onAdd}
            addSlot={<BlenderNeeded gate={gate} at="add" />}
          />
        )}
      </div>

      <BuildPanel
        ref={nameRef}
        view={view}
        placedCount={placedCount}
        hasChassis={!!chassisPart || view?.assembly.placed.some((p) => p.role === "chassis") === true}
        gate={gate}
        status={t("bike.slotsFilled", { n: slotsFilled, of: ROLES.length })}
      />

      {showMaker && (
        <Dialog open={making} onOpenChange={setMaking}>
          <DialogContent className="max-w-3xl">
            <DialogHeader>
              <DialogTitle>{t("bike.makeAPart")}</DialogTitle>
            </DialogHeader>
            <div className="max-h-[70vh] overflow-y-auto">
              <PartMaker ready={blender.ready} onChanged={changed} />
            </div>
          </DialogContent>
        </Dialog>
      )}
    </div>
    </TooltipProvider>
  );
}

/** A text field, or anything else that takes typing, has the keys. */
function isTyping(t: EventTarget | null): boolean {
  const el = t as HTMLElement | null;
  if (!el || !("tagName" in el)) return false;
  if (el.isContentEditable || el.tagName === "TEXTAREA" || el.tagName === "SELECT") return true;
  return el.tagName === "INPUT" && !["checkbox", "radio", "button", "range"].includes((el as HTMLInputElement).type);
}

/** An action-bar button that may need Blender: when it asked and there's none, the short
 *  "needs Blender" line pops up right under it, not in a bar of its own. */
function Gated({
  gate,
  at,
  children,
}: {
  gate: ReturnType<typeof useBlenderGate>;
  at: BlenderAction | BlenderAction[];
  children: React.ReactNode;
}) {
  const ats = Array.isArray(at) ? at : [at];
  const open = !!gate.asked && ats.includes(gate.asked);
  return (
    <Popover open={open} onOpenChange={(o) => !o && gate.dismiss()}>
      <PopoverAnchor asChild>
        <span className="inline-flex">{children}</span>
      </PopoverAnchor>
      <PopoverContent align="start" className="w-auto max-w-[24rem] px-3 py-2">
        <BlenderNeeded gate={gate} at={at} />
      </PopoverContent>
    </Popover>
  );
}
