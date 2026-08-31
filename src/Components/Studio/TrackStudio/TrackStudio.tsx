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
  GripVertical,
  Plus,
  Waves,
  type LucideIcon,
} from "lucide-react";
import { toast } from "sonner";

import { Button } from "../../ui/button";
import { Input } from "../../ui/input";
import { TrackViewer } from "../../Viewer/TrackViewer";
import { loadTrackOverview, loadTrackTerrain } from "../../../api/tracks";
import type { TrackOverview, TrackTerrain } from "../../../types";
import { useT } from "../../../i18n/context";
import { cn } from "@/lib/utils";
import {
  baseTrackProgram,
  buildTrack,
  checkTrack,
  exportTrackSource,
  generateTrack,
  installTrackPreview,
  lapLength,
  FEATURE_COLOUR,
  featureSpan,
  fitFeatures,
  lapSteps,
  newFeature,
  positionAt,
  previewTrack,
  roomiestGap,
  setTrackTools,
  trackToolsStatus,
  type BuildStep,
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
  const [busy, setBusy] = useState<
    "generate" | "preview" | "install" | "export" | "build" | null
  >(null);
  const [program, setProgram] = useState<TrackProgram | null>(null);
  const [preview, setPreview] = useState<TrackPreview | null>(null);
  const [problems, setProblems] = useState<string[]>([]);
  const [terrain, setTerrain] = useState<TrackTerrain | null>(null);
  const [overview, setOverview] = useState<TrackOverview | null>(null);
  const [focus, setFocus] = useState<{ x: number; z: number } | null>(null);
  const [hover, setHover] = useState<{ x: number; z: number; width: number } | null>(null);
  // Reordering is done with pointer events, not HTML5 drag-and-drop. Tauri's
  // `dragDropEnabled` hands drags to the OS so the webview never sees a dragstart — which is
  // also why the whole-window file dropzone was catching every attempt.
  const listRef = useRef<HTMLOListElement>(null);
  const [dragging, setDragging] = useState<number | null>(null);
  const [dropAt, setDropAt] = useState<number | null>(null);
  const [tools, setTools] = useState<TrackToolsStatus | null>(null);
  const [steps, setSteps] = useState<BuildStep[]>([]);

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
        const found = await checkTrack(next);
        setProblems(found);
        return found;
      } catch (e) {
        setProblems([String(e)]);
        return [String(e)];
      }
    },
    [],
  );

  async function onGenerate() {
    if (!brief.trim() || busy) return;
    setBusy("generate");
    setPreview(null);
    setProblems([]);
    try {
      const next = await generateTrack(brief.trim());
      await settle(next);
      toast.success(t("track.generated", { name: next.name }));
    } catch (e) {
      toast.error(t("track.generateFailed"), { description: String(e) });
    } finally {
      setBusy(null);
    }
  }

  async function onBase() {
    if (busy) return;
    setBusy("generate");
    setPreview(null);
    setProblems([]);
    try {
      const next = await baseTrackProgram();
      await settle(next);
      toast.success(t("track.baseLoaded", { name: next.name }));
    } catch (e) {
      toast.error(t("track.generateFailed"), { description: String(e) });
    } finally {
      setBusy(null);
    }
  }

  async function onPreview() {
    if (!program || busy) return;
    setBusy("preview");
    try {
      const p = await previewTrack(program);
      setPreview(p);
      // Terrain first so something is on screen, then the surfaces that colour the features
      // — the second is the slower half and the view is useful before it lands.
      const t3 = await loadTrackTerrain(p.path, 1024);
      setTerrain(t3);
      setOverview(await loadTrackOverview(p.path, 2048).catch(() => null));
    } catch (e) {
      toast.error(t("track.buildFailed"), { description: String(e) });
    } finally {
      setBusy(null);
    }
  }

  async function onInstall() {
    if (!program || busy) return;
    setBusy("install");
    try {
      const path = await installTrackPreview(program);
      toast.success(t("track.installed"), { description: path });
    } catch (e) {
      toast.error(t("track.installFailed"), { description: String(e) });
    } finally {
      setBusy(null);
    }
  }

  async function onExport() {
    if (!program || busy) return;
    const dir = await openDialog({ multiple: false, directory: true });
    if (typeof dir !== "string") return;
    setBusy("export");
    try {
      const wrote = await exportTrackSource(program, dir);
      toast.success(t("track.exported", { count: wrote.length }), { description: dir });
    } catch (e) {
      toast.error(t("track.exportFailed"), { description: String(e) });
    } finally {
      setBusy(null);
    }
  }

  function editFeature(index: number, patch: Partial<TrackFeature>) {
    if (!program) return;
    const features = program.features.map((f, i) =>
      i === index ? ({ ...f, ...patch } as TrackFeature) : f,
    );
    void settle({ ...program, features });
  }

  function editSegment(index: number, patch: Partial<TrackSegment>) {
    if (!program) return;
    const segments = program.segments.map((seg, i) =>
      i === index ? ({ ...seg, ...patch } as TrackSegment) : seg,
    );
    // Shortening a corner can leave the jumps beyond it hanging off the end of the lap.
    void settle(fitFeatures({ ...program, segments }));
  }

  function removeFeature(index: number) {
    if (!program) return;
    void settle({ ...program, features: program.features.filter((_, i) => i !== index) });
  }

  /// A corner or a straight can go too — the lap stops closing, and the validator says so
  /// in metres, which is a better teacher than a disabled button.
  function removeSegment(index: number) {
    if (!program || program.segments.length <= 2) return;
    void settle(
      fitFeatures({ ...program, segments: program.segments.filter((_, i) => i !== index) }),
    );
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
  function gapUnder(clientY: number): number {
    const rows = Array.from(listRef.current?.children ?? []) as HTMLElement[];
    for (let i = 0; i < rows.length; i++) {
      const box = rows[i].getBoundingClientRect();
      if (clientY < box.top + box.height / 2) return i;
    }
    return rows.length;
  }

  function onGripDown(e: React.PointerEvent, row: number) {
    e.preventDefault();
    e.stopPropagation();
    (e.target as HTMLElement).setPointerCapture(e.pointerId);
    setDragging(row);
    setDropAt(row);
  }

  function onGripMove(e: React.PointerEvent) {
    if (dragging === null) return;
    setDropAt(gapUnder(e.clientY));
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

  function addFeature(kind: TrackFeatureKind) {
    if (!program) return;
    const probe = newFeature(kind, 0);
    const at = roomiestGap(program, featureSpan(probe).length);
    void settle({ ...program, features: [...program.features, newFeature(kind, at)] });
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

  async function onBuild() {
    if (!program || busy) return;
    const dir = await openDialog({ multiple: false, directory: true });
    if (typeof dir !== "string") return;
    setBusy("build");
    setSteps([]);
    try {
      const ran = await buildTrack(program, dir);
      setSteps(ran);
      const failed = ran.find((s) => !s.ok);
      if (failed) {
        toast.error(t("track.buildStepFailed", { step: failed.name }), {
          description: failed.output.slice(0, 400),
        });
      } else {
        toast.success(t("track.compiled"), { description: dir });
      }
    } catch (e) {
      toast.error(t("track.compileFailed"), { description: String(e) });
    } finally {
      setBusy(null);
    }
  }

  const blocked = problems.length > 0;

  return (
    <div className="flex h-full min-h-0 flex-col gap-4 px-7 pb-6">
      {/* The brief. One line, because a track is described in a sentence and the schema does
          the rest of the work. */}
      <div className="flex flex-none items-center gap-2">
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
        {/* Always here, not just when the model is unreachable: starting from a track that
            already works and changing two jumps is a better first move than describing one
            from nothing. */}
        <Button
          variant="outline"
          onClick={() => void onBase()}
          disabled={busy !== null}
          className="h-10 flex-none"
        >
          {t("track.base")}
        </Button>
      </div>

      {!program && busy === null && (
        <p className="flex-none text-[12.5px] leading-snug text-muted-foreground">
          {t("track.sequenceHint")}
        </p>
      )}

      {busy === "generate" && (
        <p className="flex-none text-[12.5px] text-muted-foreground">{t("track.generatingHint")}</p>
      )}

      {!program && busy !== "generate" && (
        <div className="flex flex-1 items-center justify-center">
          <p className="max-w-md text-center text-[13px] leading-relaxed text-muted-foreground">
            {t("track.empty")}
          </p>
        </div>
      )}

      {program && (
        <div className="flex min-h-0 flex-1 gap-4">
          {/* Left: what the lap is, and what it measures. */}
          <div className="flex w-[300px] flex-none flex-col gap-3">
            <div className="rounded-xl border border-input p-3.5">
              {/* The name is the folder, the .pkz and what the game lists it as, so it is
                  worth being able to change before any of those are written. */}
              <input
                value={program.name}
                onChange={(e) => void settle({ ...program, name: e.target.value })}
                className="w-full rounded-md bg-transparent text-[15px] font-bold tracking-[-0.2px] outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
                aria-label={t("track.name")}
              />
              <input
                value={program.author}
                onChange={(e) => void settle({ ...program, author: e.target.value })}
                placeholder={t("track.author")}
                className="mt-0.5 w-full rounded-md bg-transparent text-[12px] text-muted-foreground outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
                aria-label={t("track.author")}
              />
              <dl className="mt-2.5 grid grid-cols-2 gap-x-3 gap-y-1.5 text-[12.5px]">
                <Row label={t("track.lap")} value={`${lapLength(program).toFixed(0)} m`} />
                <Row label={t("track.width")} value={`${program.width.toFixed(1)} m`} />
                <Row label={t("track.ground")} value={`${program.terrain.sizeX} × ${program.terrain.sizeZ} m`} />
                <Row label={t("track.features")} value={String(program.features.length)} />
                <Row label={t("track.corners")} value={String(program.segments.filter((s) => s.kind === "arc").length)} />
              </dl>
            </div>

            {/* Measured, not claimed — the same figures taken of published tracks. */}
            {preview && (
              <div className="rounded-xl border border-input p-3.5">
                <h3 className="text-[12px] font-semibold uppercase tracking-wide text-muted-foreground">
                  {t("track.measured")}
                </h3>
                <dl className="mt-2 grid grid-cols-2 gap-x-3 gap-y-1.5 text-[12.5px]">
                  <Row label={t("track.lap")} value={`${preview.measuredLengthM.toFixed(0)} m`} />
                  <Row label={t("track.width")} value={`${preview.measuredWidthM.toFixed(1)} m`} />
                  <Row label={t("track.lips")} value={`${preview.lips} · ${preview.lipsPerKm.toFixed(0)}/km`} />
                  <Row label={t("track.steepest")} value={`${preview.slopeP99Deg.toFixed(0)}°`} />
                  <Row label={t("track.relief")} value={`${preview.reliefP90M.toFixed(2)} m`} />
                  <Row
                    label={t("track.budget")}
                    value={`${preview.usedM.toFixed(1)} / ${preview.budgetM.toFixed(0)} m`}
                  />
                </dl>
              </div>
            )}

            {problems.length > 0 && (
              <div className="rounded-xl border border-destructive/40 bg-destructive/[0.06] p-3.5">
                <h3 className="text-[12px] font-semibold uppercase tracking-wide text-destructive">
                  {t("track.problems")}
                </h3>
                <ul className="mt-2 space-y-1.5 text-[12.5px] leading-snug text-foreground/90">
                  {problems.map((p, i) => (
                    <li key={i}>{p}</li>
                  ))}
                </ul>
              </div>
            )}

            <div className="mt-auto flex flex-col gap-2">
              <Button onClick={() => void onPreview()} disabled={blocked || busy !== null}>
                {busy === "preview" ? t("track.building") : t("track.preview")}
              </Button>
              <div className="flex gap-2">
                <Button
                  variant="outline"
                  className="flex-1"
                  onClick={() => void onInstall()}
                  disabled={blocked || busy !== null}
                >
                  {t("track.install")}
                </Button>
                <Button
                  variant="outline"
                  className="flex-1"
                  onClick={() => void onExport()}
                  disabled={blocked || busy !== null}
                >
                  {t("track.export")}
                </Button>
              </div>
              {tools?.found ? (
                <Button
                  variant="outline"
                  onClick={() => void onBuild()}
                  disabled={blocked || busy !== null}
                >
                  {busy === "build" ? t("track.compiling") : t("track.compile")}
                </Button>
              ) : (
                <Button variant="ghost" onClick={() => void onPointAtTools()} disabled={busy !== null}>
                  {t("track.pointAtTools")}
                </Button>
              )}
              {steps.length > 0 && (
                <ul className="space-y-1 text-[11.5px] leading-snug">
                  {steps.map((s) => (
                    <li key={s.name} className={s.ok ? "text-muted-foreground" : "text-destructive"}>
                      {s.ok ? "✓" : "✕"} {s.name}
                      {s.produced ? ` → ${s.produced}` : ""}
                    </li>
                  ))}
                </ul>
              )}
              <p className="text-[11.5px] leading-snug text-muted-foreground">
                {tools?.found ? t("track.stillNeeded") : t("track.previewOnly")}
              </p>
            </div>
          </div>

          {/* Middle: the lap in the order you ride it — a straight, a left turn, a double.
              Corners and jumps live in different lists in the program, but nobody rides them
              that way, so here they are one sequence. */}
          <div className="flex min-h-0 w-[420px] flex-none flex-col rounded-xl border border-input">
            {/* Adding one is picking what it is; where it goes is the emptiest stretch of
                lap, because dropping it at the finish usually lands it on something. */}
            <div className="flex flex-none flex-wrap items-center gap-1 border-b border-input px-2 py-1.5">
              <Plus className="size-3.5 flex-none text-muted-foreground" />
              {(Object.keys(FEATURE_ICON) as TrackFeatureKind[]).map((kind) => (
                <button
                  key={kind}
                  onClick={() => addFeature(kind)}
                  disabled={busy !== null}
                  className="cursor-default rounded px-1.5 py-1 text-[11.5px] text-muted-foreground transition-colors hover:bg-foreground/[0.06] hover:text-foreground disabled:opacity-40"
                >
                  {t(KIND_KEY[kind])}
                </button>
              ))}
            </div>
            <ol ref={listRef} className="min-h-0 flex-1 divide-y divide-input/60 overflow-y-auto">
              {lapSteps(program).map((step, row) => {
                const Icon = stepIcon(step);
                const steps = lapSteps(program);
                return (
                  <li
                    key={row}
                    onClick={() => setFocus(positionAt(program, step.at))}
                    onPointerEnter={() =>
                      setHover({
                        // The middle of the feature, not its start — a 40 m berm marked at
                        // its entry looks like it belongs to the corner before it.
                        ...positionAt(
                          program,
                          step.kind === "feature"
                            ? step.at + featureSpan(step.feature).length / 2
                            : step.at,
                        ),
                        width: program.width * 1.6,
                      })
                    }
                    onPointerLeave={() => setHover(null)}
                    className={cn(
                      "relative flex items-center gap-1.5 px-2 py-1.5 text-[12.5px] transition-colors",
                      terrain && "cursor-default hover:bg-foreground/[0.04]",
                      dragging === row && "opacity-40",
                      // The line lands above this row when it is the drop target, and below
                      // the last one when the drop is past the end.
                      dropAt === row && "before:absolute before:inset-x-2 before:top-0 before:h-0.5 before:bg-primary",
                      dropAt === steps.length &&
                        row === steps.length - 1 &&
                        "after:absolute after:inset-x-2 after:bottom-0 after:h-0.5 after:bg-primary",
                    )}
                    style={
                      step.kind === "feature"
                        ? { boxShadow: `inset 3px 0 0 ${FEATURE_COLOUR[step.feature.kind]}` }
                        : undefined
                    }
                  >
                    <GripVertical
                      onPointerDown={(e) => onGripDown(e, row)}
                      onPointerMove={onGripMove}
                      onPointerUp={() => onGripUp(steps)}
                      className="size-3.5 flex-none cursor-default text-faint hover:text-foreground"
                    />
                    <span className="w-11 flex-none tabular-nums text-right text-muted-foreground">
                      {step.at.toFixed(0)}
                    </span>
                    <Icon
                      className={cn(
                        "size-4 flex-none",
                        step.kind !== "feature" && "text-muted-foreground",
                      )}
                      style={
                        step.kind === "feature"
                          ? { color: FEATURE_COLOUR[step.feature.kind] }
                          : undefined
                      }
                    />
                    <span className="w-[104px] flex-none truncate">{stepName(step, t)}</span>

                    <div className="flex flex-1 flex-wrap items-center gap-x-3 gap-y-1">
                      {fieldsOf(step).map((f) => (
                        <Field
                          key={f.label}
                          label={f.label}
                          value={f.value}
                          step={f.step}
                          onChange={(v) =>
                            step.kind === "feature"
                              ? editFeature(step.index, { [f.key]: v } as Partial<TrackFeature>)
                              : editSegment(step.index, {
                                  [f.key]:
                                    f.key === "radius" ? (step.kind === "left" ? -v : v) : v,
                                } as Partial<TrackSegment>)
                          }
                        />
                      ))}
                    </div>

                    <button
                      onClick={() =>
                        step.kind === "feature"
                          ? removeFeature(step.index)
                          : removeSegment(step.index)
                      }
                      className="rounded px-1.5 text-muted-foreground transition-colors hover:bg-foreground/[0.06] hover:text-foreground"
                      aria-label={t("common.delete")}
                    >
                      ×
                    </button>
                  </li>
                );
              })}
            </ol>
          </div>

          {/* Right: the track itself. Features are painted with a colour each, so a row in
              the list and a lump on the ground can be matched up by eye. */}
          <div className="relative min-h-0 min-w-0 flex-1 overflow-hidden rounded-xl border border-input bg-black/20">
            {terrain ? (
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
          </div>
        </div>
      )}

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
} as const;

const FEATURE_ICON: Record<TrackFeature["kind"], LucideIcon> = {
  tabletop: Square,
  double: ChevronsUp,
  roller: Waves,
  whoops: Activity,
  stepUp: TrendingUp,
  berm: Spline,
  rut: Minus,
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
function fieldsOf(step: LapStep): { key: string; label: string; value: number; step: number }[] {
  const len = (v: number) => ({ key: "length", label: "m", value: v, step: 1 });
  const rise = (v: number) => ({ key: "rise", label: "↕", value: v, step: 0.5 });
  if (step.kind === "straight") {
    const seg = step.segment as { length: number; rise: number };
    return [len(seg.length), rise(seg.rise ?? 0)];
  }
  if (step.kind === "left" || step.kind === "right") {
    const seg = step.segment as { radius: number; angle: number; rise: number };
    return [
      // Signed on the wire — positive turns right — but shown as the radius you would
      // measure, because the arrow already says which way it goes.
      { key: "radius", label: "r", value: Math.abs(seg.radius), step: 1 },
      { key: "angle", label: "°", value: seg.angle, step: 5 },
      rise(seg.rise ?? 0),
    ];
  }
  const f = step.feature;
  switch (f.kind) {
    case "double":
      return [
        { key: "height", label: "h", value: f.height, step: 0.1 },
        { key: "gap", label: "gap", value: f.gap, step: 1 },
        { key: "lip", label: "lip", value: f.lip, step: 0.5 },
      ];
    case "whoops":
      return [
        { key: "height", label: "h", value: f.height, step: 0.05 },
        { key: "count", label: "×", value: f.count, step: 1 },
        { key: "spacing", label: "gap", value: f.spacing, step: 0.5 },
      ];
    case "rut":
      return [
        { key: "depth", label: "deep", value: f.depth, step: 0.05 },
        len(f.length),
      ];
    default:
      return [{ key: "height", label: "h", value: f.height, step: 0.1 }, len(f.length)];
  }
}

/** A labelled number, small enough that several fit on a row. */
function Field({
  label,
  value,
  step,
  onChange,
}: {
  label: string;
  value: number;
  step: number;
  onChange: (v: number) => void;
}) {
  return (
    <label className="flex items-center gap-1" onClick={(e) => e.stopPropagation()}>
      <span className="text-[11px] text-muted-foreground">{label}</span>
      <Num value={value} step={step} onChange={onChange} />
    </label>
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
}: {
  value: number;
  step: number;
  onChange: (v: number) => void;
}) {
  const [draft, setDraft] = useState<string | null>(null);
  return (
    <input
      type="number"
      step={step}
      value={draft ?? value}
      onChange={(e) => setDraft(e.target.value)}
      onBlur={() => {
        const n = Number(draft);
        if (draft !== null && draft !== "" && Number.isFinite(n)) onChange(n);
        setDraft(null);
      }}
      className={cn(
        "w-[58px] rounded-md border border-input bg-transparent px-1.5 py-0.5 tabular-nums",
        "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60",
      )}
    />
  );
}
