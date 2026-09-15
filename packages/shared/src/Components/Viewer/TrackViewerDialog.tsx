import { useEffect, useState } from "react";
import { Boxes, Check, Copy, Gamepad2, Loader2, Minus, Mountain, X } from "lucide-react";
import { Dialog, DialogClose, DialogContent } from "../ui/dialog";
import { Button } from "../ui/button";
import { TrackViewer, type PickedPiece } from "./TrackViewer";
import { STEPS, useTrackScene, type StepState } from "./useTrackScene";
import { diagnoseTrack } from "../../api/tracks";
import { onViewerSourceChanged, watchViewerSource } from "../../api/mods";
import { formatLength } from "../../lib/mods";
import { useT } from "../../i18n/context";

/** Metres. Below this, `[info] length` is a placeholder rather than a lap. */
const MIN_PLAUSIBLE_LENGTH = 50;

/** Spelled out rather than built from the step name, so the keys stay greppable. */
const STEP_LABEL = {
  terrain: "trackViewer.step.terrain",
  sky: "trackViewer.step.sky",
  ground: "trackViewer.step.ground",
  scenery: "trackViewer.step.scenery",
  colours: "trackViewer.step.colours",
} as const;

/** How long the finished list stays up before it fades, in ms. */
const DONE_LINGER_MS = 1600;

/** One pass in the sequence: what it is, and where it has got to. */
function LoadStep({ label, state }: { label: string; state: StepState }) {
  // A dash rather than a tick for `none`: the pass finished and the track carries nothing,
  // which is a different fact from the pass having painted something.
  const mark = {
    waiting: <span className="size-3 rounded-full border border-white/25" />,
    running: <Loader2 className="size-3 animate-spin text-white/80" />,
    done: <Check className="size-3 text-emerald-400" />,
    none: <Minus className="size-3 text-white/35" />,
    failed: <X className="size-3 text-red-400" />,
  }[state];
  return (
    <div className="flex items-center gap-2 text-[11px] leading-4">
      <span className="flex size-3 flex-none items-center justify-center">{mark}</span>
      <span className={state === "waiting" ? "text-white/40" : "text-white/80"}>{label}</span>
    </div>
  );
}

interface TrackViewerDialogProps {
  open: boolean;
  onOpenChange: (o: boolean) => void;
  /** The track's `.pkz` or unpacked folder. */
  path: string;
  title?: string;
}

export function TrackViewerDialog({
  open,
  onOpenChange,
  path,
  title,
}: TrackViewerDialogProps) {
  const t = useT();
  // On by default: the scenery is the difference between a shape and a place, and a track
  // that carries none simply has nothing to switch off.
  const [showObjects, setShowObjects] = useState(true);
  // Off by default: the view darkens the hollows so relief reads; this shows exactly the game.
  const [gameView, setGameView] = useState(false);
  // Kept up briefly after the last pass settles, so "done" is something you see happen
  // rather than the absence of a spinner.
  const [showSteps, setShowSteps] = useState(false);
  // What the last click on the scenery landed on, so the panel can name it.
  const [picked, setPicked] = useState<PickedPiece | null>(null);
  // Only fetched when a track fails, and only when asked for — it re-reads the archive.
  const [diagnosis, setDiagnosis] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  // Ticks when the track changes on disk, to load it again.
  const [generation, setGeneration] = useState(0);

  // Redraw when the track is rebuilt on disk — a regenerated track, a re-export — rather
  // than leaving the old one up until the viewer is closed and opened again.
  useEffect(() => {
    if (!open) return;
    void watchViewerSource(path);
    const pending = onViewerSourceChanged((e) => {
      if (e.source === path) setGeneration((g) => g + 1);
    });
    return () => {
      void pending.then((un) => un());
      void watchViewerSource(null);
    };
  }, [open, path]);

  const {
    info,
    terrain,
    overview,
    scenery,
    surfaces,
    backdrop,
    ground,
    groundLayers,
    placements,
    loading,
    refining,
    painting,
    steps,
    settled,
    sceneryError,
    error,
  } = useTrackScene(open ? path : null, { generation });

  // A fresh load starts the panel over: nothing picked, no stale diagnosis, the list back up.
  useEffect(() => {
    if (!open) return;
    setPicked(null);
    setDiagnosis(null);
    setCopied(false);
    setShowSteps(true);
  }, [open, path, generation]);

  // Hold the finished list up for a moment, then take it away. Without the pause the last
  // tick and the list vanishing happen in the same frame, so the thing you were waiting to
  // see never appears.
  useEffect(() => {
    if (!settled || !showSteps) return;
    const id = setTimeout(() => setShowSteps(false), DONE_LINGER_MS);
    return () => clearTimeout(id);
  }, [settled, showSteps]);

  const meta = info?.meta;
  const rows: [string, string][] = [];
  if (meta?.author) rows.push([t("libraryDetail.author"), meta.author]);
  if (meta?.location) rows.push([t("libraryDetail.location"), meta.location]);
  // Track builders leave `length` at a placeholder more often than they fill it in — a
  // published track in hand states `length = 1` while its own `.rdf` puts the finish line at
  // 97 m. A lap shorter than this isn't a motocross track, so say nothing rather than "1 m".
  if (meta?.length && meta.length >= MIN_PLAUSIBLE_LENGTH) {
    rows.push([t("libraryDetail.length"), formatLength(meta.length)]);
  }
  if (meta?.altitude != null) rows.push([t("libraryDetail.altitude"), `${meta.altitude} m`]);
  if (terrain) {
    rows.push([t("trackViewer.grid"), `${terrain.width} × ${terrain.height}`]);
    // Only when the height file said what its samples mean. Otherwise the grid is in raw
    // quantised units and "65535 m of elevation" would be worse than saying nothing.
    if (terrain.heightsInMetres) {
      rows.push([
        t("trackViewer.relief"),
        `${Math.round(terrain.maxHeight - terrain.minHeight)} m`,
      ]);
    }
  }

  if (overview) rows.push([t("trackViewer.surface"), t("trackViewer.surfaceMasks")]);

  const markerCount = placements.filter((p) => p.kind !== "prop").length;
  const hasObjects = scenery != null || markerCount > 0;
  if (scenery) {
    rows.push([
      t("trackViewer.scenery"),
      t("trackViewer.sceneryTris", {
        count: Math.round(scenery.indices.length / 3).toLocaleString(),
      }),
    ]);
  }
  if (scenery && scenery.pieceCount > 0) {
    rows.push([t("trackViewer.pieces"), scenery.pieceCount.toLocaleString()]);
  }
  if (markerCount > 0) {
    rows.push([t("trackViewer.fixtures"), String(markerCount)]);
  }
  if (picked) {
    const [w, h, d] = picked.size.map((n) => (n < 10 ? n.toFixed(1) : Math.round(n)));
    rows.push([
      t("trackViewer.selected"),
      `${picked.triangles.toLocaleString()} · ${w} × ${h} × ${d} m`,
    ]);
  }

  // Nothing to load rather than something that failed — worth saying plainly, since the
  // inventory can tell us before the terrain read even finishes.
  const noTerrain = info != null && !info.hasTerrain;

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        showClose={false}
        className="flex h-[85vh] w-[92vw] max-w-none flex-col gap-0 overflow-hidden p-0 sm:max-w-none"
      >
        <div className="flex flex-none items-center justify-between gap-3 border-b border-border px-4 py-2.5">
          <div className="flex min-w-0 items-center gap-2 text-sm font-medium">
            <Mountain className="h-4 w-4 flex-none text-muted-foreground" />
            <span className="truncate">{title ?? meta?.name ?? t("trackViewer.title")}</span>
            {(refining || painting) && (
              <span className="flex-none text-[11px] font-normal text-muted-foreground">
                {painting ? t("trackViewer.painting") : t("trackViewer.refining")}
              </span>
            )}
          </div>
          <div className="flex flex-none items-center gap-2">
            {/* Only offered once there is something to draw. A track with no scenery and no
                fixtures would otherwise get a control that does nothing. */}
            {hasObjects && (
              <Button
                variant={showObjects ? "outline" : "ghost"}
                size="sm"
                className="h-7 gap-1.5 px-2 text-[12px]"
                aria-pressed={showObjects}
                onClick={() => setShowObjects((v) => !v)}
              >
                <Boxes className="size-3.5" />
                {t("trackViewer.objects")}
              </Button>
            )}
            {groundLayers.length > 0 && (
              <Button
                variant={gameView ? "outline" : "ghost"}
                size="sm"
                className="h-7 gap-1.5 px-2 text-[12px]"
                aria-pressed={gameView}
                onClick={() => setGameView((v) => !v)}
              >
                <Gamepad2 className="size-3.5" />
                {t("trackViewer.gameView")}
              </Button>
            )}
            <DialogClose className="rounded-md p-1 text-muted-foreground opacity-70 transition-opacity hover:opacity-100 focus:outline-none">
              <X className="size-4" />
              <span className="sr-only">{t("common.close")}</span>
            </DialogClose>
          </div>
        </div>

        <div className="flex min-h-0 flex-1">
          <div className="relative min-w-0 flex-1">
            <TrackViewer
              terrain={terrain}
              overview={overview}
              scenery={scenery}
              surfaces={surfaces}
              backdrop={backdrop}
              ground={ground}
              groundLayers={groundLayers}
              placements={placements}
              showObjects={showObjects}
              gameView={gameView}
              onPick={setPicked}
              className="absolute inset-0"
            />
            {loading && !terrain && (
              <div className="pointer-events-none absolute inset-0 flex flex-col items-center justify-center gap-3 bg-black/40">
                <Loader2 className="h-6 w-6 animate-spin text-white/80" />
                <span className="text-sm text-white/80">{t("trackViewer.loading")}</span>
              </div>
            )}
            {/* The passes, as they land. Sits in the corner rather than over the track: the
                terrain is up long before the colours are, and a track you can already turn
                should not be behind a modal spinner while its sheets inflate. */}
            {showSteps && (
              <div
                className={`pointer-events-none absolute bottom-3 left-3 flex flex-col gap-1 rounded-md border border-white/10 bg-black/55 px-3 py-2 backdrop-blur-sm transition-opacity duration-500 ${
                  settled ? "opacity-0" : "opacity-100"
                }`}
                aria-live="polite"
              >
                {STEPS.map((s) => (
                  <LoadStep key={s} label={t(STEP_LABEL[s])} state={steps[s]} />
                ))}
                <div className="mt-1 border-t border-white/10 pt-1 text-[11px] font-medium text-white/90">
                  {settled ? t("trackViewer.stepsDone") : t("trackViewer.stepsBusy")}
                </div>
              </div>
            )}
            {/* Only when there's nothing on screen. A refine pass that failed leaves the
                coarse terrain up and working, and covering it with an error would be a lie. */}
            {!loading && !terrain && (error || noTerrain) && (
              <div className="pointer-events-none absolute inset-0 flex flex-col items-center justify-center gap-1 px-6 text-center">
                <span className="text-sm font-medium text-foreground">
                  {t("trackViewer.noTerrain")}
                </span>
                <span className="max-w-md text-xs text-muted-foreground">
                  {t("trackViewer.noTerrainHint")}
                </span>
                {/* The format is undocumented, so a track that won't load is evidence. This
                    puts that evidence in reach of whoever is holding the track, who is
                    rarely the person who can rebuild the app to go and look. */}
                <div className="pointer-events-auto mt-3 flex flex-col items-center gap-2">
                  {!diagnosis ? (
                    <Button
                      variant="outline"
                      size="sm"
                      onClick={() => {
                        void diagnoseTrack(path)
                          .then(setDiagnosis)
                          .catch((e) =>
                            setDiagnosis(e instanceof Error ? e.message : String(e)),
                          );
                      }}
                    >
                      {t("trackViewer.whyDetails")}
                    </Button>
                  ) : (
                    <>
                      <pre className="max-h-56 w-[min(46rem,80vw)] overflow-auto rounded-md border border-border bg-background/80 p-3 text-left text-[11px] leading-relaxed">
                        {diagnosis}
                      </pre>
                      <Button
                        variant="outline"
                        size="sm"
                        onClick={() => {
                          void navigator.clipboard.writeText(diagnosis).then(() => {
                            setCopied(true);
                            setTimeout(() => setCopied(false), 2000);
                          });
                        }}
                      >
                        {copied ? (
                          <Check className="size-3.5" />
                        ) : (
                          <Copy className="size-3.5" />
                        )}
                        {copied ? t("trackViewer.copied") : t("trackViewer.copyDetails")}
                      </Button>
                    </>
                  )}
                </div>
              </div>
            )}
          </div>

          <aside className="w-64 flex-none overflow-y-auto border-l border-border p-4">
            {meta?.thumbnail && (
              <img
                src={meta.thumbnail}
                alt=""
                className="mb-3 w-full rounded-md border border-border object-cover"
              />
            )}
            <dl className="space-y-2">
              {rows.map(([label, value]) => (
                <div key={label}>
                  <dt className="text-[10px] font-semibold uppercase tracking-[0.9px] text-faint">
                    {label}
                  </dt>
                  <dd className="text-[13px] text-foreground">{value}</dd>
                </div>
              ))}
            </dl>

            {sceneryError && (
              <p className="mt-4 border-t border-border pt-3 text-[11px] leading-relaxed text-destructive">
                {sceneryError}
              </p>
            )}

            {/* The height file has no documented layout, so what's on screen was worked out
                from the data. Say so, rather than letting a guess pass for a reading. */}
            {terrain && (terrain.confidence < 0.75 || !terrain.scaleKnown) && (
              <p className="mt-4 border-t border-border pt-3 text-[11px] leading-relaxed text-muted-foreground">
                {!terrain.scaleKnown
                  ? t("trackViewer.assumedScaleNote")
                  : t("trackViewer.inferredNote")}
              </p>
            )}
          </aside>
        </div>
      </DialogContent>
    </Dialog>
  );
}
