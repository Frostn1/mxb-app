import { useCallback, useEffect, useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { toast } from "sonner";

import { Button } from "../../ui/button";
import { Input } from "../../ui/input";
import { TrackViewerDialog } from "../../Viewer/TrackViewerDialog";
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
  lapSteps,
  previewTrack,
  setTrackTools,
  trackToolsStatus,
  type BuildStep,
  type LapStep,
  type TrackFeature,
  type TrackPreview,
  type TrackProgram,
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
  const [viewing, setViewing] = useState(false);
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
      setViewing(true);
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

  function removeFeature(index: number) {
    if (!program) return;
    void settle({ ...program, features: program.features.filter((_, i) => i !== index) });
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
              <h2 className="text-[15px] font-bold tracking-[-0.2px]">{program.name}</h2>
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

          {/* Right: the lap in the order you ride it — a straight, a left turn, a double.
              Corners and jumps live in different lists in the program, but nobody rides them
              that way, so here they are one sequence. */}
          <div className="min-h-0 flex-1 overflow-y-auto rounded-xl border border-input">
            <ol className="divide-y divide-input/60">
              {lapSteps(program).map((step, row) => (
                <li key={row} className="flex items-center gap-3 px-3.5 py-2 text-[12.5px]">
                  <span className="w-14 flex-none tabular-nums text-right text-muted-foreground">
                    {step.at.toFixed(0)} m
                  </span>
                  <span className="flex-1">{describe(step, t)}</span>
                  {step.kind === "feature" && (
                    <>
                      <Num
                        value={
                          step.feature.kind === "rut" ? step.feature.depth : step.feature.height
                        }
                        step={0.1}
                        onChange={(v) =>
                          editFeature(
                            step.index,
                            (step.feature.kind === "rut"
                              ? { depth: v }
                              : { height: v }) as Partial<TrackFeature>,
                          )
                        }
                      />
                      <button
                        onClick={() => removeFeature(step.index)}
                        className="rounded px-1.5 text-muted-foreground hover:bg-foreground/[0.06] hover:text-foreground"
                        aria-label={t("common.delete")}
                      >
                        ×
                      </button>
                    </>
                  )}
                  {step.kind !== "feature" && <span className="w-[92px] flex-none" />}
                </li>
              ))}
            </ol>
          </div>
        </div>
      )}

      {preview && (
        <TrackViewerDialog
          open={viewing}
          onOpenChange={setViewing}
          path={preview.path}
          title={preview.name}
        />
      )}
    </div>
  );
}

/**
 * One step of the lap, in words.
 *
 * Numbers carry their own units and need no translating, so only the noun is a key. The
 * shape is deliberately flat — "Left turn · 18 m radius, 75°" reads at a glance, and reads
 * the same way a brief is written.
 */
const KIND_KEY = {
  tabletop: "track.kind.tabletop",
  double: "track.kind.double",
  roller: "track.kind.roller",
  whoops: "track.kind.whoops",
  stepUp: "track.kind.stepUp",
  berm: "track.kind.berm",
  rut: "track.kind.rut",
} as const;

function describe(step: LapStep, t: ReturnType<typeof useT>): string {
  const m = (v: number, digits = 0) => `${v.toFixed(digits)} m`;
  switch (step.kind) {
    case "straight":
      return `${t("track.straight")} · ${m(step.length)}`;
    case "left":
    case "right":
      return `${t(step.kind === "left" ? "track.turnLeft" : "track.turnRight")} · ${m(
        step.radius,
      )} ${t("track.radius")}, ${step.angle.toFixed(0)}°`;
    case "feature": {
      const f = step.feature;
      const name = t(KIND_KEY[f.kind]);
      switch (f.kind) {
        case "double":
          return `${name} · ${m(f.height, 1)}, ${m(f.gap)} ${t("track.gap")}`;
        case "whoops":
          return `${name} · ${f.count} × ${m(f.spacing, 1)}`;
        case "rut":
          return `${name} · ${m(f.depth, 2)} ${t("track.deep")}`;
        case "stepUp":
          return `${name} · ${m(f.height, 1)}`;
        default:
          return `${name} · ${m(f.height, 1)} ${t("track.over")} ${m(f.length)}`;
      }
    }
  }
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
        "w-[68px] rounded-md border border-input bg-transparent px-1.5 py-0.5 tabular-nums",
        "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60",
      )}
    />
  );
}
