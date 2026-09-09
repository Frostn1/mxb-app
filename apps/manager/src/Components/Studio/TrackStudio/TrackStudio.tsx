import { useCallback, useEffect, useRef, useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import {
  Activity,
  ChevronsUp,
  CornerUpLeft,
  CornerUpRight,
  Minus,
  MoveRight,
  Spline,
  Square,
  TrendingDown,
  TrendingUp,
  ChevronDown,
  ChevronRight,
  GripVertical,
  Maximize2,
  Minimize2,
  PenLine,
  RefreshCw,
  Waves,
  type LucideIcon,
} from "lucide-react";
import { toast } from "sonner";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "../../ui/alert-dialog";

import { Button } from "../../ui/button";
import { Input } from "../../ui/input";
import { TrackViewer } from "../../Viewer/TrackViewer";
import BuildCard from "./BuildCard";
import LapPlan from "./LapPlan";
import ElevationCurve from "./ElevationCurve";
import { Switch } from "../../ui/switch";
import { Segmented } from "../../ui/segmented";
import { loadTrackOverview, loadTrackTerrain } from "../../../api/tracks";
import type { TrackOverview, TrackTerrain } from "../../../types";
import { useT } from "../../../i18n/context";
import { isRunning, useTrackBuild } from "../../../Context/TrackBuild";
import { cn } from "@/lib/utils";
import {
  baseTrackProgram,
  blankTrackProgram,
  randomTrackProgram,
  closeTrackLap,
  fitTrackBudget,
  checkTrack,
  exportTrackSource,
  generateTrack,
  lapLength,
  FEATURE_COLOUR,
  elevationAt,
  featureMiddle,
  featureSpan,
  fitFeatures,
  lapSteps,
  newFeature,
  pathAlong,
  positionAt,
  setElevationAt,
  previewTrack,
  roomiestGap,
  setTrackTools,
  trackToolsStatus,
  type LapStep,
  type TrackFeature,
  type TrackFeatureKind,
  type TrackPreview,
  type TrackProgram,
  type TrackSegment,
  type TrackToolsStatus,
} from "../../../api/trackgen";

/**
 * Track Studio: describe a track, get a track.
 *
 * The thing on screen is the *program* — a lap of straights and arcs with jumps laid along
 * it — and not the terrain, because the program is the part worth editing. Change a jump's
 * height here and it is one number; change it in a heightmap and it is a sculpting job.
 *
 * Nothing is taken on trust. Every program, generated or hand-edited, is built and measured
 * against what published tracks measure before it can be previewed, and the complaints are
 * shown as they come back rather than being swallowed into "invalid".
 */
export default function TrackStudio() {
  const t = useT();
  const [brief, setBrief] = useState("");
  const [working, setWorking] = useState<"generate" | "preview" | "export" | null>(null);
  // The build itself lives above this component — see `Context/TrackBuild` — so that leaving
  // the tab doesn't take the bar with it. To everything here that asks "is the studio busy?"
  // it is still one answer.
  const { build, start: startBuild } = useTrackBuild();
  const building = build !== null && isRunning(build.state);
  const busy: "generate" | "preview" | "export" | "build" | null = building
    ? "build"
    : working;
  const [program, setProgram] = useState<TrackProgram | null>(null);
  const [preview, setPreview] = useState<TrackPreview | null>(null);
  const [problems, setProblems] = useState<string[]>([]);
  const [notes, setNotes] = useState<string[]>([]);
  // Whether anything has been changed since it was loaded, so a starting point can't be
  // dropped on top of an afternoon's work by accident.
  const [touched, setTouched] = useState(false);
  const [confirming, setConfirming] = useState<(() => Promise<void>) | null>(null);
  const [terrain, setTerrain] = useState<TrackTerrain | null>(null);
  // Building a 2049-square terrain is seconds of work with nothing on screen to say so, and
  // with Live on it happened silently — you moved a number, the picture didn't change, and
  // the studio looked broken until it caught up.
  const [rebuilding, setRebuilding] = useState(false);
  // The program the terrain on screen was built from. A 3D view of an older lap is the same
  // picture as a 3D view that has not updated, and only this can tell them apart.
  const [built, setBuilt] = useState<string | null>(null);
  const [overview, setOverview] = useState<TrackOverview | null>(null);
  const [focus, setFocus] = useState<{ x: number; z: number } | null>(null);
  // The viewer, filling the window. It is the same element either way — only the wrapper's
  // classes change — because remounting it would throw away the scene and rebuild the
  // terrain from scratch every time you toggled.
  const [full, setFull] = useState(false);
  const [hover, setHover] = useState<{
    path: { x: number; z: number }[];
    width: number;
  } | null>(null);
  // The same gesture in the form the plan wants. The 3D viewer highlights a path of world
  // points; the plan re-walks the lap itself, so it only needs to be told which stretch.
  const [hoverSpan, setHoverSpan] = useState<{ at: number; length: number } | null>(null);
  // Reordering is done with pointer events, not HTML5 drag-and-drop. Tauri's
  // `dragDropEnabled` hands drags to the OS so the webview never sees a dragstart — which is
  // also why the whole-window file dropzone was catching every attempt.
  const listRef = useRef<HTMLOListElement>(null);
  const [dragging, setDragging] = useState<number | null>(null);
  const [dropAt, setDropAt] = useState<number | null>(null);
  // Collapsed by segment index. A lap is a run of corners and straights with jumps on them,
  // so the segment is the section — no new field, and it is the grouping people already
  // have in their heads.
  const [shut, setShut] = useState<Set<number>>(new Set());
  const [flash, setFlash] = useState<number | null>(null);
  // Which row the height strip is showing. A corner or a straight at a time, because on a
  // 1500 m lap a 40 m berm is three pixels wide and every point lands on the last one.
  const [scope, setScope] = useState<number | null>(null);
  const [stripMode, setStripMode] = useState<"height" | "shape">("height");
  // Which picture the stage is showing. The plan is drawn from the program and costs
  // nothing, so it is what you get until you ask for the ground itself.
  const [stage, setStage] = useState<"plan" | "solid">("plan");
  // The two things the left column's footer can open, one at a time — a row of feature
  // kinds, or the brief. Both are one line and neither is worth a dialog.
  const [adding, setAdding] = useState(false);
  const [asking, setAsking] = useState(false);
  // Rebuilding a two-thousand-square terrain on every drag is real work, so this is a choice
  // rather than the default. With it on, an edit settles and then the view catches up.
  const [live, setLive] = useState(false);
  const rebuild = useRef<ReturnType<typeof setTimeout> | null>(null);
  const [tools, setTools] = useState<TrackToolsStatus | null>(null);

  // Whether the compilers are here decides whether the last step is a button or a folder of
  // homework, so it is worth knowing before anyone has generated anything.
  useEffect(() => {
    trackToolsStatus().then(setTools).catch(() => {});
  }, []);

  /** Re-check and re-measure. Called after every edit, so the numbers are never stale. */
  const settle = useCallback(
    async (next: TrackProgram) => {
      setProgram(next);
      setPreview(null);
      try {
        let found = await checkTrack(next);
        // The height budget is arithmetic, not a decision. Work it out and carry on rather
        // than telling someone to raise a number they have no field for.
        if (found.problems.some((p) => p.includes("budget"))) {
          try {
            next = await fitTrackBudget(next);
            setProgram(next);
            found = await checkTrack(next);
          } catch {
            /* leave the original complaint standing */
          }
        }
        setProblems(found.problems);
        setNotes(found.notes);
        if (live && found.problems.length === 0) {
          // Debounced: a drag is a hundred edits, and only the last one is worth building.
          if (rebuild.current) clearTimeout(rebuild.current);
          const settled = next;
          rebuild.current = setTimeout(() => void showIn3d(settled).catch(() => {}), 500);
        }
        return found.problems;
      } catch (e) {
        setProblems([String(e)]);
        setNotes([]);
        return [String(e)];
      }
    },
    [live],
  );

  async function onGenerate() {
    if (!brief.trim() || busy) return;
    setWorking("generate");
    setPreview(null);
    setProblems([]);
    try {
      const next = await generateTrack(brief.trim());
      await settle(next);
      setAsking(false);
      toast.success(t("track.generated", { name: next.name }));
    } catch (e) {
      toast.error(t("track.generateFailed"), { description: String(e) });
    } finally {
      setWorking(null);
    }
  }

  /** Load a starting point and put it on screen — a track you can't see isn't a start. */
  async function onLoad(load: () => Promise<TrackProgram>) {
    if (busy) return;
    // Replacing a track you have been working on is the one action here that throws work
    // away, so it asks first — and only when there is work to throw away.
    if (touched) {
      setConfirming(() => () => reallyLoad(load));
      return;
    }
    await reallyLoad(load);
  }

  async function reallyLoad(load: () => Promise<TrackProgram>) {
    if (busy) return;
    setWorking("generate");
    setPreview(null);
    setProblems([]);
    try {
      const next = await load();
      const found = await settle(next);
      setTouched(false);
      toast.success(t("track.baseLoaded", { name: next.name }));
      if (found.length === 0) await showIn3d(next);
    } catch (e) {
      toast.error(t("track.generateFailed"), { description: String(e) });
    } finally {
      setWorking(null);
    }
  }

  // Escape leaves full screen. Anything that covers the whole window has to have a way out
  // that does not involve finding a button on top of a 3D scene.
  useEffect(() => {
    if (!full) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setFull(false);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [full]);

  // A track that goes away while you are looking at it full screen would leave you staring
  // at an empty black window with no obvious way back.
  useEffect(() => {
    if (!terrain) setFull(false);
  }, [terrain]);

  async function onClose() {
    if (!program || busy) return;
    try {
      const next = await closeTrackLap(program);
      await settle(next);
      toast.success(t("track.lapClosed"));
    } catch (e) {
      toast.error(t("track.closeFailed"), { description: String(e) });
    }
  }

  async function showIn3d(prog: TrackProgram) {
    setRebuilding(true);
    try {
      const p = await previewTrack(prog);
      setPreview(p);
      const t3 = await loadTrackTerrain(p.path, 1024);
      setTerrain(t3);
      // 1024, not 2048: the surface picture is a texture on a preview, and at the
      // larger size it is seventeen megabytes over the bridge every time the track is
      // rebuilt — which with Live on is every edit.
      setOverview(await loadTrackOverview(p.path, 1024).catch(() => null));
      setBuilt(JSON.stringify(prog));
    } finally {
      setRebuilding(false);
    }
  }

  async function onPreview() {
    if (!program || busy) return;
    setWorking("preview");
    setStage("solid");
    try {
      await showIn3d(program);
    } catch (e) {
      toast.error(t("track.buildFailed"), { description: String(e) });
    } finally {
      setWorking(null);
    }
  }

  async function onExport() {
    if (!program || busy) return;
    const dir = await openDialog({ multiple: false, directory: true });
    if (typeof dir !== "string") return;
    setWorking("export");
    try {
      const wrote = await exportTrackSource(program, dir);
      toast.success(t("track.exported", { count: wrote.length }), { description: dir });
    } catch (e) {
      toast.error(t("track.exportFailed"), { description: String(e) });
    } finally {
      setWorking(null);
    }
  }

  function editFeature(index: number, patch: Partial<TrackFeature>) {
    if (!program) return;
    setTouched(true);
    const features = program.features.map((f, i) =>
      i === index ? ({ ...f, ...patch } as TrackFeature) : f,
    );
    void settle({ ...program, features });
  }

  function editSegment(index: number, patch: Partial<TrackSegment>) {
    if (!program) return;
    setTouched(true);
    const segments = program.segments.map((seg, i) =>
      i === index ? ({ ...seg, ...patch } as TrackSegment) : seg,
    );
    // Shortening a corner can leave the jumps beyond it hanging off the end of the lap.
    void settle(fitFeatures({ ...program, segments }));
  }

  function removeFeature(index: number) {
    if (!program) return;
    setTouched(true);
    void settle({ ...program, features: program.features.filter((_, i) => i !== index) });
  }

  /// A corner or a straight can go too — the lap stops closing, and the validator says so
  /// in metres, which is a better teacher than a disabled button.
  function removeSegment(index: number) {
    if (!program || program.segments.length <= 2) return;
    setTouched(true);
    // Everything past it comes back with it. A jump is placed by how far round the lap it
    // is, so taking a 120 m corner out from under one and leaving its number alone moves it
    // 120 m further round — off the straight it was built for and into the next corner.
    const at = segmentStart(program.segments, index);
    const gone = segLength(program.segments[index]);
    const features = program.features.map((f) =>
      f.at >= at + gone ? { ...f, at: Math.max(0, f.at - gone) } : f,
    );
    void settle(
      fitFeatures({
        ...program,
        segments: program.segments.filter((_, i) => i !== index),
        features,
      }),
    );
  }

  /**
   * Put a new piece of lap in, after the one you are looking at.
   *
   * A lap is built by carrying on from where you are, not by always appending to the end —
   * so a new corner lands after the selected step, and the selection follows it there. It
   * will open the lap up, which the checks say in metres with a button to close it again;
   * that is the same conversation as any other edit that moves the finish.
   */
  function addSegment(kind: "straight" | "left" | "right") {
    if (!program) return;
    setTouched(true);
    const on = scope !== null ? steps[scope] : undefined;
    // A feature belongs to the segment it sits on, so adding from a jump's row adds after
    // that jump's piece of track.
    let index = program.segments.length;
    if (on) {
      if (on.kind !== "feature") index = on.index + 1;
      else
        for (let i = scope ?? 0; i >= 0; i--)
          if (steps[i].kind !== "feature") {
            index = steps[i].index + 1;
            break;
          }
    }
    const seg: TrackSegment =
      kind === "straight"
        ? { kind: "straight", length: NEW_STRAIGHT_M, rise: 0 }
        : {
            kind: "arc",
            // Signed radius: negative turns left. Sized inside what published tracks run.
            radius: kind === "left" ? -NEW_RADIUS_M : NEW_RADIUS_M,
            angle: NEW_ARC_DEG,
            rise: 0,
          };
    const segments = [...program.segments];
    segments.splice(index, 0, seg);
    const at = segmentStart(program.segments, index);
    const grew = segLength(seg);
    const features = program.features.map((f) => (f.at >= at ? { ...f, at: f.at + grew } : f));
    const next = fitFeatures({ ...program, segments, features });
    void settle(next);
    // Select it and put it on screen, or a new row appears somewhere off the bottom of a
    // thirty-step list and reads as nothing having happened.
    const row = lapSteps(next).findIndex((x) => x.kind !== "feature" && x.index === index);
    if (row >= 0) {
      setScope(row);
      setFocus(positionAt(next, at));
    }
    setFlash(at);
  }

  /**
   * Move a row to where it was dropped.
   *
   * The two halves of a lap are stored differently — corners and straights are an ordered
   * list, features are placed by how far round they are — so a drop means two different
   * things depending on what was dragged. Reordering segments changes the shape of the lap;
   * moving a feature only changes where on it the jump sits.
   */
  function reorder(steps: LapStep[], from: number, to: number) {
    if (!program || from === to) return;
    setTouched(true);
    const moved = steps[from];
    const target = steps[to];
    if (moved.kind === "feature") {
      const at = target.kind === "feature" ? target.at : target.at + (to > from ? 1 : 0);
      const features = program.features.map((f, i) =>
        i === moved.index ? { ...f, at: Math.max(0, at) } : f,
      );
      void settle({ ...program, features });
      return;
    }
    // A segment lands where the row it was dropped on sits. Dropped on a feature, that is
    // the segment the feature is on — the last one that starts at or before it.
    const landing =
      target.kind === "feature"
        ? steps.filter((x) => x.kind !== "feature" && x.at <= target.at).length - 1
        : target.index;
    const next = [...program.segments];
    const [seg] = next.splice(moved.index, 1);
    next.splice(Math.min(Math.max(landing, 0), next.length), 0, seg);
    void settle(fitFeatures({ ...program, segments: next }));
  }

  /** Which gap between rows the pointer is over. */
  // Bring a newly added feature into view once the list has it.
  useEffect(() => {
    if (flash === null || !listRef.current) return;
    const row = listRef.current.querySelector<HTMLElement>(`[data-at="${flash}"]`);
    row?.scrollIntoView({ block: "center", behavior: "smooth" });
    const id = setTimeout(() => setFlash(null), 1400);
    return () => clearTimeout(id);
  }, [flash, program]);

  function gapUnder(clientY: number, total: number): number {
    const rows = Array.from(
      listRef.current?.querySelectorAll<HTMLElement>("[data-step]") ?? [],
    );
    for (const row of rows) {
      const box = row.getBoundingClientRect();
      if (clientY < box.top + box.height / 2) return Number(row.dataset.step);
    }
    return total;
  }

  function onGripDown(e: React.PointerEvent, row: number) {
    e.preventDefault();
    e.stopPropagation();
    (e.target as HTMLElement).setPointerCapture(e.pointerId);
    setDragging(row);
    setDropAt(row);
  }

  function onGripMove(e: React.PointerEvent, total: number) {
    if (dragging === null) return;
    setDropAt(gapUnder(e.clientY, total));
  }

  function onGripUp(steps: LapStep[]) {
    if (dragging !== null && dropAt !== null) {
      // A gap index is one past the row above it, so dropping below where you started
      // lands one row too far without this.
      reorder(steps, dragging, Math.max(0, dropAt > dragging ? dropAt - 1 : dropAt));
    }
    setDragging(null);
    setDropAt(null);
  }

  function settleTerrain(patch: Partial<TrackProgram["terrain"]>) {
    if (!program) return;
    setTouched(true);
    void settle({ ...program, terrain: { ...program.terrain, ...patch } });
  }

  /** Put the ground under one feature at a given height, by moving the lap's height curve. */
  function liftFeature(f: TrackFeature, height: number) {
    if (!program) return;
    setTouched(true);
    void settle(setElevationAt(program, featureMiddle(f), height));
  }

  function addFeature(kind: TrackFeatureKind) {
    if (!program) return;
    setTouched(true);
    const probe = newFeature(kind, 0);
    const at = roomiestGap(program, featureSpan(probe).length);
    void settle({ ...program, features: [...program.features, newFeature(kind, at)] });
    // Put it on screen. It lands in the emptiest stretch of lap, which is rarely the part
    // you are looking at — a new row appearing somewhere off-screen reads as nothing
    // happening at all.
    setFlash(at);
  }

  async function onPointAtTools() {
    const dir = await openDialog({ multiple: false, directory: true });
    if (typeof dir !== "string") return;
    try {
      const next = await setTrackTools(dir);
      setTools(next);
      if (!next.found) toast.error(t("track.toolsNotFound"));
    } catch (e) {
      toast.error(t("track.toolsNotFound"), { description: String(e) });
    }
  }

  // Build the whole way and put it where the game reads it. No folder to pick: the app has
  // one of its own, and a track you have to go and find afterwards isn't finished. Handed
  // over rather than awaited here: the build outlives this screen.
  function onBuild() {
    if (!program || busy) return;
    startBuild(program);
  }

  // A build fetches the compilers if this machine hasn't got them, so what the studio knows
  // about them can be out of date the moment one finishes. Keyed on the phase, not the whole
  // build: that object is replaced five times a second while the bar is moving.
  const buildState = build?.state;
  useEffect(() => {
    if (!buildState || isRunning(buildState)) return;
    trackToolsStatus()
      .then(setTools)
      .catch(() => {});
  }, [buildState]);

  const blocked = problems.length > 0;
  // The 3D view is a build, so it is only ever as new as the last one. Comparing the whole
  // program is cheap next to synthesising it, and nothing smaller is honest — every field
  // here changes the ground.
  const stale = program !== null && built !== null && built !== JSON.stringify(program);

  const steps = program ? lapSteps(program) : [];
  const selected = scope !== null ? steps[scope] : undefined;
  const lap = program ? lapLength(program) : 0;

  /** Which step covers a distance round the lap — how a click on the map becomes a row. */
  function pickAt(at: number) {
    let row = 0;
    for (let i = 0; i < steps.length; i++) if (steps[i].kind !== "feature" && steps[i].at <= at) row = i;
    setScope(row);
    setFocus(positionAt(program!, at));
  }

  return (
    <div className="flex h-full min-h-0 flex-col">
      {!program ? (
        /* Nothing loaded yet. One line describes a track and the schema does the rest, and
           two starting points sit beside it for when the model isn't the answer. */
        <div className="flex min-h-0 flex-1 items-center justify-center px-7">
          <div className="w-full max-w-[560px]">
            <div className="flex items-center gap-2.5">
              <span className="u-skew h-3 w-1 bg-primary" />
              <h2 className="font-cond text-[13px] font-bold uppercase tracking-[0.2em] text-foreground">
                {t("track.briefTitle")}
              </h2>
            </div>
            <div className="mt-3 flex items-center gap-2">
              <Input
                value={brief}
                onChange={(e) => setBrief(e.target.value)}
                onKeyDown={(e) => e.key === "Enter" && void onGenerate()}
                placeholder={t("track.briefPlaceholder")}
                className="h-10"
                disabled={busy !== null}
              />
              <Button
                onClick={() => void onGenerate()}
                disabled={!brief.trim() || busy !== null}
                className="h-10 flex-none"
              >
                {busy === "generate" ? t("track.generating") : t("track.generate")}
              </Button>
            </div>
            <p className="mt-3 text-[12.5px] leading-relaxed text-muted-foreground">
              {busy === "generate" ? t("track.generatingHint") : t("track.empty")}
            </p>
            <div className="mt-4 flex items-center gap-2">
              <Button
                variant="outline"
                onClick={() => void onLoad(randomTrackProgram)}
                disabled={busy !== null}
              >
                {t("track.random")}
              </Button>
              <Button
                variant="outline"
                onClick={() => void onLoad(baseTrackProgram)}
                disabled={busy !== null}
              >
                {t("track.base")}
              </Button>
              <Button
                variant="ghost"
                onClick={() => void onLoad(blankTrackProgram)}
                disabled={busy !== null}
              >
                {t("track.blank")}
              </Button>
            </div>
          </div>
        </div>
      ) : (
        <div className="flex min-h-0 flex-1">
          {/* ── The program: the lap in the order you ride it ──────────────────
              Corners and jumps live in different lists in the program, but nobody rides
              them that way, so here they are one numbered sequence. */}
          <aside className="flex w-[268px] flex-none flex-col border-r border-border">
            <div className="flex flex-none items-center gap-2.5 px-4 pb-2.5 pt-4">
              <span className="u-skew h-3 w-1 bg-primary" />
              <h2 className="flex-1 font-cond text-[13px] font-bold uppercase tracking-[0.2em] text-foreground">
                {t("track.program")}
              </h2>
              <span className="tabular-figures font-cond text-[11px] text-faint">
                {steps.length}
              </span>
            </div>

            <ol ref={listRef} className="min-h-0 flex-1 overflow-y-auto">
              {steps.map((step, row) => {
                const Icon = stepIcon(step);
                // Which section this row belongs to: the last segment at or before it.
                let section = -1;
                for (let i = row; i >= 0; i--) {
                  if (steps[i].kind !== "feature") {
                    section = steps[i].index;
                    break;
                  }
                }
                const collapsed = shut.has(section);
                if (step.kind === "feature" && collapsed) return null;
                const inSection =
                  step.kind !== "feature"
                    ? steps.filter((x, i) => {
                        if (x.kind !== "feature") return false;
                        for (let j = i; j >= 0; j--)
                          if (steps[j].kind !== "feature") return steps[j].index === step.index;
                        return false;
                      }).length
                    : 0;
                return (
                  <li
                    key={row}
                    data-step={row}
                    data-at={step.at}
                    onClick={() => {
                      setScope(row);
                      setFocus(positionAt(program, step.at));
                    }}
                    onPointerEnter={() => {
                      // The whole of it, not where it starts: a straight is two hundred
                      // metres long and its first metre says nothing about which one it is.
                      const length = stepLength(step);
                      setHoverSpan({ at: step.at, length });
                      setHover({
                        path: pathAlong(program, step.at, length),
                        width: program.width * 1.6,
                      });
                    }}
                    onPointerLeave={() => {
                      setHover(null);
                      setHoverSpan(null);
                    }}
                    className={cn(
                      "relative flex h-[42px] cursor-default items-center gap-2 border-b border-border/50 px-3 transition-colors",
                      "hover:bg-foreground/[0.04]",
                      dragging === row && "opacity-40",
                      scope === row && "bg-card",
                      dropAt === row &&
                        "before:absolute before:inset-x-0 before:top-0 before:h-0.5 before:bg-primary",
                      dropAt === steps.length &&
                        row === steps.length - 1 &&
                        "after:absolute after:inset-x-0 after:bottom-0 after:h-0.5 after:bg-primary",
                      flash !== null && step.at === flash && "bg-primary-tint",
                    )}
                  >
                    {scope === row && (
                      <span className="u-skew absolute left-0 top-2 h-[26px] w-[3px] bg-primary" />
                    )}
                    <span className="w-[22px] flex-none tabular-figures text-right font-cond text-[11px] font-bold text-faint">
                      {String(row + 1).padStart(2, "0")}
                    </span>
                    <Icon
                      className={cn(
                        "size-3.5 flex-none",
                        step.kind !== "feature" && "text-muted-foreground",
                      )}
                      style={
                        step.kind === "feature"
                          ? { color: FEATURE_COLOUR[step.feature.kind] }
                          : undefined
                      }
                    />
                    <div className="min-w-0 flex-1">
                      <div
                        className={cn(
                          "truncate font-cond text-[12.5px] font-semibold uppercase tracking-[0.12em]",
                          scope === row ? "text-primary" : "text-muted-foreground",
                        )}
                        style={
                          step.kind === "feature" && scope !== row
                            ? { color: FEATURE_COLOUR[step.feature.kind] }
                            : undefined
                        }
                      >
                        {stepName(step, t)}
                      </div>
                      {/* The numbers, read not typed. Typing them is the right-hand panel's
                          job, and thirty rows of input boxes is a form, not a lap. */}
                      <div className="truncate font-mono text-[10.5px] tabular-figures text-faint">
                        {summarise(step, t)}
                      </div>
                    </div>
                    {step.kind !== "feature" && (
                      <button
                        onClick={(e) => {
                          e.stopPropagation();
                          setShut((prev) => {
                            const next = new Set(prev);
                            if (!next.delete(step.index)) next.add(step.index);
                            return next;
                          });
                        }}
                        className="flex-none cursor-default text-faint hover:text-foreground"
                        aria-expanded={!collapsed}
                        aria-label={stepName(step, t)}
                      >
                        {collapsed ? (
                          <span className="flex items-center gap-1">
                            {inSection > 0 && (
                              <span className="tabular-figures text-[10.5px]">{inSection}</span>
                            )}
                            <ChevronRight className="size-3.5" />
                          </span>
                        ) : (
                          <ChevronDown className="size-3.5" />
                        )}
                      </button>
                    )}
                    <GripVertical
                      onPointerDown={(e) => onGripDown(e, row)}
                      onPointerMove={(e) => onGripMove(e, steps.length)}
                      onPointerUp={() => onGripUp(steps)}
                      className="size-3.5 flex-none cursor-default text-faint hover:text-foreground"
                    />
                  </li>
                );
              })}
            </ol>

            {/* Adding one is picking what it is; where it goes is the emptiest stretch of
                lap, because dropping it at the finish usually lands it on something. */}
            {adding && (
              <div className="flex-none border-t border-border px-3 py-2">
                {/* Two groups, because they are two different things. The first changes the
                    shape of the lap; the second lays something on the shape it already has. */}
                <div className="font-cond text-[10px] font-semibold uppercase tracking-[0.22em] text-faint">
                  {t("track.groupLap")}
                </div>
                <div className="mt-1.5 flex flex-wrap gap-1">
                  {(
                    [
                      ["straight", "track.straight", MoveRight],
                      ["left", "track.turnLeft", CornerUpLeft],
                      ["right", "track.turnRight", CornerUpRight],
                    ] as const
                  ).map(([kind, key, Icon]) => (
                    <button
                      key={kind}
                      onClick={() => {
                        addSegment(kind);
                        setAdding(false);
                      }}
                      disabled={busy !== null}
                      className="flex cursor-default items-center gap-1.5 border border-border px-2 py-1 font-cond text-[11px] font-semibold uppercase tracking-[0.12em] text-muted-foreground transition-colors hover:border-primary hover:text-foreground disabled:opacity-40"
                    >
                      <Icon className="size-3" />
                      {t(key)}
                    </button>
                  ))}
                </div>
                <div className="mt-2.5 font-cond text-[10px] font-semibold uppercase tracking-[0.22em] text-faint">
                  {t("track.groupOnIt")}
                </div>
                <div className="mt-1.5 flex flex-wrap gap-1">
                  {(Object.keys(FEATURE_ICON) as TrackFeatureKind[]).map((kind) => (
                    <button
                      key={kind}
                      onClick={() => {
                        addFeature(kind);
                        setAdding(false);
                      }}
                      disabled={busy !== null}
                      className="cursor-default border border-border px-2 py-1 font-cond text-[11px] font-semibold uppercase tracking-[0.12em] text-muted-foreground transition-colors hover:border-primary hover:text-foreground disabled:opacity-40"
                      style={{ color: FEATURE_COLOUR[kind] }}
                    >
                      {t(KIND_KEY[kind])}
                    </button>
                  ))}
                </div>
              </div>
            )}
            {asking && (
              <div className="flex flex-none items-center gap-2 border-t border-border px-3 py-2">
                <Input
                  autoFocus
                  value={brief}
                  onChange={(e) => setBrief(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") void onGenerate();
                    if (e.key === "Escape") setAsking(false);
                  }}
                  placeholder={t("track.briefPlaceholder")}
                  className="h-8 text-[12px]"
                  disabled={busy !== null}
                />
              </div>
            )}
            <div className="flex flex-none items-center gap-3 border-t border-border px-3 py-2.5">
              <button
                onClick={() => {
                  setAdding((v) => !v);
                  setAsking(false);
                }}
                disabled={busy !== null}
                className={cn(
                  "flex-1 cursor-default text-left font-cond text-[11px] font-semibold uppercase tracking-[0.16em] transition-colors disabled:opacity-40",
                  adding ? "text-foreground" : "text-primary hover:text-foreground",
                )}
              >
                {t("track.addStep")}
              </button>
              <button
                onClick={() => {
                  setAsking((v) => !v);
                  setAdding(false);
                }}
                disabled={busy !== null}
                className={cn(
                  "cursor-default font-cond text-[11px] font-semibold uppercase tracking-[0.16em] transition-colors disabled:opacity-40",
                  asking ? "text-foreground" : "text-muted-foreground hover:text-foreground",
                )}
              >
                {busy === "generate" ? t("track.generating") : t("track.generate")}
              </button>
            </div>
          </aside>

          {/* ── The lap itself, and its height ────────────────────────────────
              The plan is drawn from the program, so it is always current; the 3D view is a
              build, so it is a thing you ask for. */}
          <section className="flex min-w-0 flex-1 flex-col">
            <div
              className={cn(
                "relative min-h-0 flex-1 overflow-hidden bg-window",
                full && "fixed inset-0 z-50 bg-black",
              )}
            >
              {stage === "plan" && !full ? (
                <LapPlan
                  program={program}
                  scope={
                    selected
                      ? { at: selected.at, length: stepLength(selected) }
                      : null
                  }
                  hover={hoverSpan}
                  onPick={pickAt}
                  className="absolute inset-0"
                />
              ) : terrain ? (
                <TrackViewer
                  terrain={terrain}
                  overview={overview}
                  scenery={null}
                  surfaces={[]}
                  backdrop={null}
                  ground={null}
                  placements={[]}
                  showObjects={false}
                  focus={focus}
                  highlight={hover}
                  className="absolute inset-0"
                />
              ) : (
                <div className="absolute inset-0 grid place-items-center px-6 text-center text-[12.5px] text-muted-foreground">
                  {busy === "preview" ? t("track.building") : t("track.previewHint")}
                </div>
              )}

              {/* Why the ground is not what the numbers say. Two different answers — one is
                  "wait", the other is "press the button" — and telling them apart is the
                  whole difference between a slow studio and a broken one. */}
              {stage === "solid" && !full && terrain && (
                <div className="absolute left-4 top-4 flex items-center gap-2">
                  {rebuilding ? (
                    <span className="flex items-center gap-2 bg-black/55 px-2.5 py-1 backdrop-blur">
                      <RefreshCw className="size-3 animate-spin text-white/90" />
                      <span className="font-cond text-[10.5px] font-semibold uppercase tracking-[0.16em] text-white/90">
                        {t("track.building")}
                      </span>
                    </span>
                  ) : (
                    stale && (
                      <button
                        onClick={() => void onPreview()}
                        disabled={blocked || busy !== null}
                        className="flex cursor-default items-center gap-2 border-l-2 border-warning bg-black/55 px-2.5 py-1 backdrop-blur disabled:opacity-50"
                      >
                        <span className="font-cond text-[10.5px] font-semibold uppercase tracking-[0.16em] text-warning">
                          {t("track.stale")}
                        </span>
                      </button>
                    )
                  )}
                </div>
              )}

              {/* What the colours on the map mean. */}
              {stage === "plan" && !full && (
                <div className="pointer-events-none absolute left-4 top-4 flex items-center gap-4">
                  <Legend className="bg-primary" label={t("track.legendSelected")} />
                  <Legend className="bg-warning" label={t("track.legendJump")} />
                  <Legend className="bg-primary/30" label={t("track.legendLap")} />
                </div>
              )}

              {/* Measured off the program, not claimed. */}
              {!full && (
                <div className="pointer-events-none absolute right-4 top-4 flex items-start gap-4">
                  <Stat value={(lap / 1000).toFixed(2)} unit="km" label={t("track.lap")} />
                  <Stat value={String(steps.length)} label={t("track.steps")} />
                  <Stat value={climbOf(program).toFixed(0)} unit="m" label={t("track.climb")} />
                </div>
              )}

              <div className="absolute bottom-3 right-3 flex items-center gap-2">
                {!full && (
                  <Segmented
                    size="sm"
                    value={stage}
                    onChange={(v) => setStage(v as "plan" | "solid")}
                    options={[
                      { value: "plan", label: t("track.plan") },
                      { value: "solid", label: t("track.in3d") },
                    ]}
                  />
                )}
                {(stage === "solid" || full) && terrain && (
                  <Button
                    variant="ghost"
                    size="icon"
                    onClick={() => setFull((v) => !v)}
                    title={t(full ? "track.exitFullscreen" : "track.fullscreen")}
                    aria-label={t(full ? "track.exitFullscreen" : "track.fullscreen")}
                    className="size-8 bg-black/45 text-white/90 backdrop-blur hover:bg-black/65"
                  >
                    {full ? <Minimize2 className="size-4" /> : <Maximize2 className="size-4" />}
                  </Button>
                )}
              </div>
            </div>

            {/* The lap's own height, as a line you can pull about — the same shape the
                segment rises describe, in the form you can take hold of. */}
            <div className="flex-none border-t border-border bg-card/40 px-4 pb-2 pt-2.5">
              <div className="flex items-center gap-2.5">
                <span className="u-skew h-2.5 w-1 bg-primary" />
                <span className="font-cond text-[11px] font-bold uppercase tracking-[0.2em] text-foreground">
                  {t("track.elevation")}
                </span>
                {selected ? (
                  <button
                    onClick={() => setScope(null)}
                    className="cursor-default font-mono text-[10.5px] text-faint hover:text-foreground"
                    title={t("track.wholeLap")}
                  >
                    {stepName(selected, t)} · {selected.at.toFixed(0)}–
                    {(selected.at + stepLength(selected)).toFixed(0)} m ×
                  </button>
                ) : (
                  <span className="font-mono text-[10.5px] text-faint">{t("track.wholeLap")}</span>
                )}
                {/* Only where there is a shape to edit: a straight has ground, not a shape. */}
                {selected?.kind === "feature" && (
                  <div className="ml-auto">
                    <Segmented
                      size="sm"
                      value={stripMode}
                      onChange={(v) => setStripMode(v as "height" | "shape")}
                      options={[
                        { value: "height", label: t("track.mode.height") },
                        { value: "shape", label: t("track.mode.shape") },
                      ]}
                    />
                  </div>
                )}
              </div>
              <ElevationCurve
                lap={lap}
                {...(() => {
                  if (!selected) return {};
                  // A little either side, so the ends of the stretch can be shaped against
                  // what they run into rather than against the edge of the picture.
                  const pad = Math.max(stepLength(selected) * 0.15, 5);
                  return {
                    from: Math.max(0, selected.at - pad),
                    to: selected.at + stepLength(selected) + pad,
                  };
                })()}
                knots={program.elevation ?? []}
                features={program.features}
                onChange={(elevation) => {
                  setTouched(true);
                  void settle({ ...program, elevation });
                }}
                onFeature={editFeature}
                {...(() => {
                  const on = selected;
                  if (!on) return {};
                  return {
                    scoped:
                      on.kind === "feature"
                        ? { at: on.at, feature: on.feature }
                        : { at: on.at, segment: on.segment },
                    onScoped: (patch: Record<string, number>) =>
                      on.kind === "feature"
                        ? editFeature(on.index, patch as Partial<TrackFeature>)
                        : editSegment(on.index, patch as Partial<TrackSegment>),
                    mode: on.kind === "feature" ? stripMode : "height",
                    // Only a jump that is already a drawn shape has loose points to move;
                    // every other kind is reshaped through its own numbers.
                    onShape: (shape: { u: number; h: number }[]) => {
                      if (on.kind !== "feature" || on.feature.kind !== "custom") return;
                      setTouched(true);
                      void settle({
                        ...program,
                        features: program.features.map((f, i) =>
                          i === on.index ? ({ ...on.feature, shape } as TrackFeature) : f,
                        ),
                      });
                    },
                  };
                })()}
                onHover={(i) => {
                  if (i === null) {
                    setHover(null);
                    setHoverSpan(null);
                    return;
                  }
                  const span = featureSpan(program.features[i]);
                  setHoverSpan(span);
                  setHover({
                    path: pathAlong(program, span.at, span.length),
                    width: program.width * 1.6,
                  });
                }}
                className="h-[96px]"
              />
            </div>
          </section>

          {/* ── What the selected step is, and what is wrong with the track ──── */}
          <aside className="flex w-[300px] flex-none flex-col overflow-y-auto border-l border-border">
            {selected ? (
              <div className="flex-none px-4 pb-4 pt-4">
                <div className="font-cond text-[13px] font-bold uppercase tracking-[0.2em] text-foreground">
                  {t("track.stepNumber", { n: String((scope ?? 0) + 1).padStart(2, "0") })} —{" "}
                  {stepName(selected, t)}
                </div>
                <div className="mt-1 font-mono text-[10.5px] text-faint">
                  {t("track.stepWrites")}
                </div>
                <div className="mt-4 space-y-3.5">
                  {fieldsOf(
                    selected,
                    selected.kind === "feature"
                      ? elevationAt(program, featureMiddle(selected.feature))
                      : undefined,
                  ).map((f) => (
                    <PropRow
                      key={f.key}
                      label={f.label}
                      value={f.value}
                      step={f.step}
                      min={f.min}
                      max={f.max}
                      unit={f.unit}
                      onChange={(v) =>
                        f.ground && selected.kind === "feature"
                          ? liftFeature(selected.feature, v)
                          : selected.kind === "feature"
                          ? editFeature(selected.index, { [f.key]: v } as Partial<TrackFeature>)
                          : editSegment(selected.index, {
                              [f.key]:
                                f.key === "radius" ? (selected.kind === "left" ? -v : v) : v,
                            } as Partial<TrackSegment>)
                      }
                    />
                  ))}
                </div>
                <button
                  onClick={() => {
                    if (selected.kind === "feature") removeFeature(selected.index);
                    else removeSegment(selected.index);
                    setScope(null);
                  }}
                  className="mt-5 cursor-default font-cond text-[11px] font-semibold uppercase tracking-[0.16em] text-muted-foreground transition-colors hover:text-destructive"
                >
                  {t("track.removeStep")}
                </button>
              </div>
            ) : (
              /* Nothing picked: the track's own settings, which are the other half of what
                 this panel is for and have to live somewhere. */
              <div className="flex-none px-4 pb-4 pt-4">
                <div className="font-cond text-[13px] font-bold uppercase tracking-[0.2em] text-foreground">
                  {t("track.trackSettings")}
                </div>
                <div className="mt-1 font-mono text-[10.5px] leading-snug text-faint">
                  {t("track.pickAStep")}
                </div>
                {/* The name is the folder, the .pkz and what the game lists it as, so it is
                    worth being able to change before any of those are written. */}
                <div className="mt-4 space-y-2">
                  <input
                    value={program.name}
                    onChange={(e) => void settle({ ...program, name: e.target.value })}
                    className="w-full border border-input bg-card px-2.5 py-1.5 text-[13.5px] font-bold outline-none focus:border-ring"
                    aria-label={t("track.name")}
                    placeholder={t("track.name")}
                  />
                  <div className="flex gap-2">
                    <input
                      value={program.author}
                      onChange={(e) => void settle({ ...program, author: e.target.value })}
                      placeholder={t("track.author")}
                      className="min-w-0 flex-1 border border-input bg-card px-2.5 py-1.5 text-[12px] text-muted-foreground outline-none focus:border-ring"
                      aria-label={t("track.author")}
                    />
                    <input
                      value={program.location}
                      onChange={(e) => void settle({ ...program, location: e.target.value })}
                      placeholder={t("track.location")}
                      className="min-w-0 flex-1 border border-input bg-card px-2.5 py-1.5 text-[12px] text-muted-foreground outline-none focus:border-ring"
                      aria-label={t("track.location")}
                    />
                  </div>
                </div>

                <div className="mt-4 space-y-3.5">
                  <PropRow
                    label={t("track.across")}
                    value={program.terrain.sizeX}
                    step={20}
                    min={100}
                    max={2000}
                    unit="m"
                    onChange={(v) =>
                      void settleTerrain({ sizeX: Math.max(100, v), sizeZ: Math.max(100, v) })
                    }
                  />
                  <PropRow
                    label={t("track.width")}
                    value={program.width}
                    step={0.5}
                    min={6}
                    max={24}
                    unit="m"
                    onChange={(v) => {
                      setTouched(true);
                      void settle({ ...program, width: Math.max(1, v) });
                    }}
                  />
                  <PropRow
                    label={t("track.hills")}
                    value={program.terrain.relief.amplitude}
                    step={1}
                    min={0}
                    max={40}
                    unit="m"
                    onChange={(v) =>
                      void settle({
                        ...program,
                        terrain: {
                          ...program.terrain,
                          relief: { ...program.terrain.relief, amplitude: Math.max(0, v) },
                        },
                      })
                    }
                  />
                  <PropRow
                    label={t("track.smoothing")}
                    value={program.blend}
                    step={0.5}
                    min={0}
                    max={8}
                    unit="m"
                    onChange={(v) => {
                      setTouched(true);
                      void settle({ ...program, blend: Math.max(0, v) });
                    }}
                  />
                  <div>
                    <div className="font-cond text-[10px] font-semibold uppercase tracking-[0.22em] text-faint">
                      {t("track.surface")}
                    </div>
                    <div className="mt-2 flex border border-border">
                      {(["soil", "sand", "grass"] as const).map((s, i) => (
                        <button
                          key={s}
                          onClick={() => void settleTerrain({ surface: s })}
                          className={cn(
                            "h-7 flex-1 cursor-default font-cond text-[11px] font-semibold uppercase tracking-[0.14em] transition-colors",
                            i > 0 && "border-l border-border",
                            program.terrain.surface === s
                              ? "bg-primary text-primary-foreground"
                              : "text-muted-foreground hover:text-foreground",
                          )}
                        >
                          {t(`track.${s}` as "track.soil")}
                        </button>
                      ))}
                    </div>
                  </div>
                </div>

                {/* Measured, not claimed — the same figures taken of published tracks. */}
                {preview && (
                  <dl className="mt-5 grid grid-cols-2 gap-x-3 gap-y-1.5 border-t border-border pt-3.5 text-[12px]">
                    <Row label={t("track.measured")} value={`${preview.measuredLengthM.toFixed(0)} m`} />
                    <Row label={t("track.width")} value={`${preview.measuredWidthM.toFixed(1)} m`} />
                    <Row label={t("track.lips")} value={`${preview.lips} · ${preview.lipsPerKm.toFixed(0)}/km`} />
                    <Row label={t("track.steepest")} value={`${preview.slopeP99Deg.toFixed(0)}°`} />
                    <Row label={t("track.relief")} value={`${preview.reliefP90M.toFixed(2)} m`} />
                    <Row
                      label={t("track.budget")}
                      value={`${preview.usedM.toFixed(1)} / ${preview.budgetM.toFixed(0)} m`}
                    />
                  </dl>
                )}

                <div className="mt-5 flex items-center gap-3 border-t border-border pt-3.5">
                  <span className="font-cond text-[10px] font-semibold uppercase tracking-[0.22em] text-faint">
                    {t("track.startOver")}
                  </span>
                  <button
                    onClick={() => void onLoad(randomTrackProgram)}
                    disabled={busy !== null}
                    className="cursor-default font-cond text-[11px] font-semibold uppercase tracking-[0.14em] text-muted-foreground hover:text-foreground disabled:opacity-40"
                  >
                    {t("track.random")}
                  </button>
                  <button
                    onClick={() => void onLoad(baseTrackProgram)}
                    disabled={busy !== null}
                    className="cursor-default font-cond text-[11px] font-semibold uppercase tracking-[0.14em] text-muted-foreground hover:text-foreground disabled:opacity-40"
                  >
                    {t("track.base")}
                  </button>
                  <button
                    onClick={() => void onLoad(blankTrackProgram)}
                    disabled={busy !== null}
                    className="cursor-default font-cond text-[11px] font-semibold uppercase tracking-[0.14em] text-muted-foreground hover:text-foreground disabled:opacity-40"
                  >
                    {t("track.blank")}
                  </button>
                </div>
              </div>
            )}

            {/* ── Checks ──────────────────────────────────────────────────────
                Problems block the build; notes only say the track is unlike a published
                one, which a blank lap always is. */}
            <div className="mt-auto flex-none border-t border-border px-4 pb-4 pt-3.5">
              <div className="flex items-center gap-2.5">
                <span
                  className={cn(
                    "u-skew h-3 w-1",
                    problems.length > 0 ? "bg-destructive" : notes.length > 0 ? "bg-warning" : "bg-success",
                  )}
                />
                <h3 className="flex-1 font-cond text-[11.5px] font-bold uppercase tracking-[0.2em] text-foreground">
                  {t("track.checks")}
                </h3>
                <span
                  className={cn(
                    "tabular-figures font-cond text-[10.5px]",
                    problems.length > 0 ? "text-destructive" : "text-faint",
                  )}
                >
                  {problems.length > 0
                    ? t("track.problemCount", { count: problems.length })
                    : notes.length > 0
                    ? t("track.noteCount", { count: notes.length })
                    : t("track.checksOk")}
                </span>
              </div>

              <div className="mt-2.5 space-y-1.5">
                {problems.map((p, i) => (
                  <p
                    key={`p${i}`}
                    className="border-l-2 border-destructive bg-destructive/[0.07] px-2.5 py-2 text-[11.5px] leading-snug text-foreground/90"
                  >
                    {p}
                  </p>
                ))}
                {problems.length === 0 &&
                  notes.map((n, i) => (
                    <p
                      key={`n${i}`}
                      className="border-l-2 border-warning bg-warning/[0.06] px-2.5 py-2 text-[11.5px] leading-snug text-muted-foreground"
                    >
                      {n}
                    </p>
                  ))}
              </div>

              {problems.some((p) => p.includes("doesn't close")) && (
                <Button
                  variant="outline"
                  size="sm"
                  className="mt-2.5 w-full"
                  onClick={() => void onClose()}
                >
                  {t("track.closeLap")}
                </Button>
              )}

              {build && (
                <div className="mt-3">
                  <BuildCard />
                </div>
              )}
            </div>
          </aside>
        </div>
      )}

      {/* ── Where the track goes ─────────────────────────────────────────────
          Building runs PiBoSo's own compilers over the exported source, fetching them first
          if this machine hasn't got them — there is no second path that produces a track the
          game will ride. */}
      {program && (
        <div className="flex h-[52px] flex-none items-center gap-3 border-t border-border bg-window px-5">
          <span
            className={cn(
              "font-cond text-[11px] font-semibold uppercase tracking-[0.18em]",
              blocked ? "text-destructive" : "text-success",
            )}
          >
            {blocked ? t("track.problemCount", { count: problems.length }) : t("track.valid")}
          </span>
          {!blocked && notes.length > 0 && (
            <>
              <span className="text-faint">/</span>
              <span className="font-cond text-[11px] font-semibold uppercase tracking-[0.18em] text-warning">
                {t("track.noteCount", { count: notes.length })}
              </span>
            </>
          )}
          {!tools?.found && (
            <button
              className="cursor-default text-[11.5px] text-muted-foreground underline-offset-2 hover:underline"
              onClick={() => void onPointAtTools()}
              disabled={busy !== null}
            >
              {t("track.pointAtTools")}
            </button>
          )}
          <div className="flex-1" />
          <label className="flex cursor-default items-center gap-2 text-[11.5px] text-muted-foreground">
            <Switch checked={live} onCheckedChange={setLive} />
            {t("track.live")}
          </label>
          <Button
            variant="ghost"
            onClick={() => void onExport()}
            disabled={blocked || busy !== null}
          >
            {t("track.export")}
          </Button>
          <Button
            variant="outline"
            onClick={() => void onPreview()}
            disabled={blocked || busy !== null}
          >
            <RefreshCw className={cn("size-3.5", busy === "preview" && "animate-spin")} />
            {t("track.preview")}
          </Button>
          <Button onClick={onBuild} disabled={blocked || busy !== null}>
            {building ? t("track.compiling") : t("track.compile")}
          </Button>
        </div>
      )}

      <AlertDialog open={confirming !== null} onOpenChange={(o) => !o && setConfirming(null)}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t("track.replaceTitle")}</AlertDialogTitle>
            <AlertDialogDescription>{t("track.replaceBody")}</AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>{t("common.cancel")}</AlertDialogCancel>
            <AlertDialogAction
              onClick={() => {
                const go = confirming;
                setConfirming(null);
                void go?.();
              }}
            >
              {t("track.replaceConfirm")}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
}

const KIND_KEY = {
  tabletop: "track.kind.tabletop",
  double: "track.kind.double",
  roller: "track.kind.roller",
  whoops: "track.kind.whoops",
  stepUp: "track.kind.stepUp",
  berm: "track.kind.berm",
  rut: "track.kind.rut",
  custom: "track.kind.custom",
} as const;

const FEATURE_ICON: Record<TrackFeature["kind"], LucideIcon> = {
  tabletop: Square,
  double: ChevronsUp,
  roller: Waves,
  whoops: Activity,
  stepUp: TrendingUp,
  berm: Spline,
  rut: Minus,
  custom: PenLine,
};

/** What the row is, at a glance. A list of thirty steps is scanned, not read. */
function stepIcon(step: LapStep): LucideIcon {
  if (step.kind === "straight") return MoveRight;
  if (step.kind === "left") return CornerUpLeft;
  if (step.kind === "right") return CornerUpRight;
  // A step-down is a step-up with a negative height, and drawing both the same way hides
  // the one thing that tells them apart.
  if (step.feature.kind === "stepUp" && step.feature.height < 0) return TrendingDown;
  return FEATURE_ICON[step.feature.kind];
}

/**
 * What a new piece of lap starts as.
 *
 * Inside what published tracks measure, and small enough that adding one doesn't throw the
 * lap across the plot: the corpus runs corners of 7–30 m radius, and a 45° arc is a quarter
 * of the way round a bend rather than a whole one.
 */
const NEW_STRAIGHT_M = 60;
const NEW_RADIUS_M = 25;
const NEW_ARC_DEG = 45;

/** How long one segment is. The arc's length falls out of its radius and angle. */
function segLength(seg: TrackSegment): number {
  return seg.kind === "straight"
    ? seg.length
    : (Math.abs(seg.radius) * Math.abs(seg.angle) * Math.PI) / 180;
}

/** How far round the lap a segment begins. */
function segmentStart(segments: TrackSegment[], index: number): number {
  let at = 0;
  for (let i = 0; i < Math.min(index, segments.length); i++) at += segLength(segments[i]);
  return at;
}

/** How much lap a step covers. */
function stepLength(step: LapStep): number {
  if (step.kind === "feature") return featureSpan(step.feature).length;
  const seg = step.segment;
  return seg.kind === "straight"
    ? seg.length
    : (Math.abs(seg.radius) * Math.abs(seg.angle) * Math.PI) / 180;
}

function stepName(step: LapStep, t: ReturnType<typeof useT>): string {
  if (step.kind === "straight") return t("track.straight");
  if (step.kind === "left") return t("track.turnLeft");
  if (step.kind === "right") return t("track.turnRight");
  return t(KIND_KEY[step.feature.kind]);
}

/**
 * The numbers that define a step, and which key on it each one writes.
 *
 * Every step is a handful of measurements and nothing else, so the row *is* the editor —
 * there is no dialog to open and nothing to remember about which field belongs to which
 * kind. `rise` is on every segment because "does this bit go up or down" is a question you
 * ask of a straight as often as of a corner.
 */
interface StepField {
  key: string;
  label: string;
  value: number;
  step: number;
  /** Where the slider's ends sit. Typing is never clamped to them — they are a shape, not a rule. */
  min: number;
  max: number;
  unit?: string;
  ground?: boolean;
}

function fieldsOf(step: LapStep, ground?: number): StepField[] {
  const len = (v: number, max = 400) =>
    ({ key: "length", label: "length", value: v, step: 1, min: 2, max, unit: "m" }) as StepField;
  const rise = (v: number) =>
    ({ key: "rise", label: "rise", value: v, step: 0.5, min: -20, max: 20, unit: "m" }) as StepField;
  if (step.kind === "straight") {
    const seg = step.segment as { length: number; rise: number };
    return [len(seg.length), rise(seg.rise ?? 0)];
  }
  if (step.kind === "left" || step.kind === "right") {
    const seg = step.segment as { radius: number; angle: number; rise: number };
    return [
      // Signed on the wire — positive turns right — but shown as the radius you would
      // measure, because the arrow already says which way it goes.
      { key: "radius", label: "radius", value: Math.abs(seg.radius), step: 1, min: 5, max: 200, unit: "m" },
      { key: "angle", label: "angle", value: seg.angle, step: 5, min: 0, max: 180, unit: "°" },
      rise(seg.rise ?? 0),
    ];
  }
  const f = step.feature;
  // Where the ground is under this feature, as opposed to how tall the feature is. Every
  // kind gets one: "this jump is three metres up" is a different question from "this jump is
  // two metres tall", and both are worth asking of the same row.
  const up: StepField = {
    key: "ground",
    label: "ground",
    value: ground ?? 0,
    step: 0.5,
    min: -20,
    max: 40,
    unit: "m",
    ground: true,
  };
  switch (f.kind) {
    case "double":
      return [
        { key: "height", label: "height", value: f.height, step: 0.1, min: 0, max: 6, unit: "m" },
        { key: "gap", label: "gap", value: f.gap, step: 1, min: 0, max: 40, unit: "m" },
        { key: "lip", label: "lip", value: f.lip, step: 0.5, min: 1, max: 15, unit: "m" },
        up,
      ];
    case "whoops":
      return [
        { key: "height", label: "height", value: f.height, step: 0.05, min: 0, max: 2, unit: "m" },
        { key: "count", label: "count", value: f.count, step: 1, min: 2, max: 20, unit: "×" },
        { key: "spacing", label: "spacing", value: f.spacing, step: 0.5, min: 1, max: 10, unit: "m" },
        up,
      ];
    case "rut":
      return [
        { key: "depth", label: "depth", value: f.depth, step: 0.05, min: 0, max: 1, unit: "m" },
        len(f.length, 120),
        up,
      ];
    case "custom":
      // A shape has no height or length to type at — it has points, and they are dragged.
      return [len(f.length, 120), up];
    case "stepUp":
      return [
        { key: "height", label: "height", value: f.height, step: 0.1, min: -6, max: 6, unit: "m" },
        len(f.length, 120),
        up,
      ];
    default:
      return [
        { key: "height", label: "height", value: f.height, step: 0.1, min: 0, max: 6, unit: "m" },
        len(f.length, 120),
        up,
      ];
  }
}

/**
 * What a row says about itself in one line of figures.
 *
 * The list is scanned, not read: thirty rows of input boxes is a form, and a lap is not a
 * form. The numbers are typed in the panel on the right, one step at a time.
 */
function summarise(step: LapStep, t: ReturnType<typeof useT>): string {
  const m = (v: number) => `${v.toFixed(v < 10 ? 1 : 0)} m`;
  if (step.kind === "straight") {
    const seg = step.segment as { length: number; rise: number };
    return m(seg.length) + (seg.rise ? ` · ${seg.rise > 0 ? "+" : ""}${seg.rise.toFixed(1)} m` : "");
  }
  if (step.kind === "left" || step.kind === "right") {
    const seg = step.segment as { radius: number; angle: number; rise: number };
    return `r ${Math.abs(seg.radius).toFixed(0)} m · ${Math.abs(seg.angle).toFixed(0)}°`;
  }
  const f = step.feature;
  switch (f.kind) {
    case "double":
      return `${t("track.gap")} ${m(f.gap)} · h ${f.height.toFixed(1)} m`;
    case "whoops":
      return `${f.count} × ${m(f.spacing)} · h ${f.height.toFixed(2)} m`;
    case "rut":
      return `${m(f.length)} · ${f.depth.toFixed(2)} m ${t("track.deep")}`;
    case "custom":
      return m(f.length);
    default:
      return `${m(f.length)} · h ${f.height.toFixed(1)} m`;
  }
}

/**
 * How much the lap climbs, end to end.
 *
 * The two halves of a track's height are stated separately — a `rise` per segment, and the
 * elevation curve laid over it — so neither on its own is the number a rider would give.
 */
function climbOf(program: TrackProgram): number {
  const lap = lapLength(program);
  if (lap <= 0) return 0;
  let lo = Infinity;
  let hi = -Infinity;
  let carried = 0;
  let at = 0;
  const seen: { at: number; h: number }[] = [{ at: 0, h: 0 }];
  for (const seg of program.segments) {
    const len =
      seg.kind === "straight"
        ? seg.length
        : (Math.abs(seg.radius) * Math.abs(seg.angle) * Math.PI) / 180;
    carried += seg.rise ?? 0;
    at += len;
    seen.push({ at, h: carried });
  }
  for (const k of seen) {
    const h = k.h + elevationAt(program, k.at);
    lo = Math.min(lo, h);
    hi = Math.max(hi, h);
  }
  for (const k of program.elevation ?? []) {
    // The knots sit between segment ends, and a peak halfway down a straight is still a peak.
    const before = seen.filter((q) => q.at <= k.at).pop();
    const h = (before?.h ?? 0) + k.height;
    lo = Math.min(lo, h);
    hi = Math.max(hi, h);
  }
  return Number.isFinite(hi - lo) ? hi - lo : 0;
}

/** One key of the plan's colour code. */
function Legend({ className, label }: { className: string; label: string }) {
  return (
    <span className="flex items-center gap-2">
      <span className={cn("h-[3px] w-2.5", className)} />
      <span className="font-cond text-[10px] font-semibold uppercase tracking-[0.16em] text-faint">
        {label}
      </span>
    </span>
  );
}

/** A measurement of the lap, over the picture of it. */
function Stat({ value, unit, label }: { value: string; unit?: string; label: string }) {
  return (
    <div className="min-w-[76px] border-t border-foreground/20 pt-[7px]">
      <div className="tabular-figures font-cond text-[25px] font-bold leading-none text-foreground">
        {value}
        {unit && <span className="text-[13px]"> {unit}</span>}
      </div>
      <div className="mt-[5px] font-cond text-[10px] font-semibold uppercase tracking-[0.2em] text-faint">
        {label}
      </div>
    </div>
  );
}

/**
 * One number of the selected step: named, pulled, and typed.
 *
 * The slider is for finding a value and the box is for stating one, and neither is enough on
 * its own — a radius is dragged until the corner looks right, and a lip is 5.5 m because
 * that is what the track it came from measured.
 */
function PropRow({
  label,
  value,
  step,
  min,
  max,
  unit,
  onChange,
}: {
  label: string;
  value: number;
  step: number;
  min: number;
  max: number;
  unit?: string;
  onChange: (v: number) => void;
}) {
  // The track's own value wins over the slider's ends: a 300 m plot with the slider stopping
  // at 200 would drag itself smaller the moment it was touched.
  const lo = Math.min(min, value);
  const hi = Math.max(max, value);
  const fill = hi > lo ? ((value - lo) / (hi - lo)) * 100 : 0;
  return (
    <div>
      <div className="font-cond text-[10px] font-semibold uppercase tracking-[0.22em] text-faint">
        {label}
      </div>
      <div className="mt-1.5 flex items-center gap-2.5">
        <input
          type="range"
          min={lo}
          max={hi}
          step={step}
          value={value}
          onChange={(e) => onChange(Number(e.target.value))}
          style={{ ["--fill" as string]: `${fill}%` }}
          className="min-w-0 flex-1"
          aria-label={label}
        />
        <Num value={value} step={step} onChange={onChange} className="w-[64px]" />
        {unit && <span className="w-3 flex-none text-[11px] text-faint">{unit}</span>}
      </div>
    </div>
  );
}

function Row({ label, value }: { label: string; value: string }) {
  return (
    <>
      <dt className="text-muted-foreground">{label}</dt>
      <dd className="tabular-nums">{value}</dd>
    </>
  );
}

/** A number you can edit without it fighting you while you type. */
function Num({
  value,
  step,
  onChange,
  className,
}: {
  value: number;
  step: number;
  onChange: (v: number) => void;
  className?: string;
}) {
  const [draft, setDraft] = useState<string | null>(null);
  return (
    <input
      type="number"
      step={step}
      // Rounded while it sits there, full precision the moment you type in it. The model
      // keeps whatever it had — blurring without editing commits nothing.
      value={draft ?? Number(value.toFixed(2))}
      onChange={(e) => setDraft(e.target.value)}
      onBlur={() => {
        const n = Number(draft);
        if (draft !== null && draft !== "" && Number.isFinite(n)) onChange(n);
        setDraft(null);
      }}
      className={cn(
        // 58px minus the native spinner left about thirty for the digits, so every radius
        // and length in the program was clipped mid-number. The spinner goes; a track
        // program is typed, not nudged one step at a time.
        "w-[76px] border border-input bg-card px-1.5 py-0.5 text-[12px] tabular-nums",
        "[appearance:textfield] [&::-webkit-inner-spin-button]:appearance-none [&::-webkit-outer-spin-button]:appearance-none",
        "outline-none focus:border-ring",
        className,
      )}
    />
  );
}
