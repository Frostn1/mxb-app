import { useEffect, useState } from "react";
import { Boxes, Check, Copy, Loader2, Mountain, X } from "lucide-react";
import { Dialog, DialogClose, DialogContent } from "../ui/dialog";
import { Button } from "@/Components/ui/button";
import { TrackViewer, type PickedPiece } from "./TrackViewer";
import {
  diagnoseTrack,
  loadTrackOverview,
  loadTrackBackdrop,
  loadTrackGround,
  loadTrackScenery,
  loadTrackSurfaces,
  loadTrackTerrain,
  readTrackInfo,
  readTrackPlacements,
} from "../../api/tracks";
import type {
  TrackInfo,
  TrackOverview,
  TrackPlacement,
  TrackBackdrop,
  TrackScenery,
  TrackSceneryTexture,
  TrackTerrain,
} from "../../types";
import { formatLength } from "../../lib/mods";
import { useT } from "../../i18n/context";

/**
 * The two passes the terrain is loaded in.
 *
 * A coarse grid is a few tens of kilobytes and builds its mesh in a frame or two, so the
 * track is on screen almost immediately; the detailed one replaces it once it arrives. Both
 * come from the same cached master in the backend, so the second pass costs a resample and
 * a transfer rather than another read of the archive.
 */
const COARSE_DIM = 128;
const FINE_DIM = 2048;

/** The surface picture's longest edge. Matches the masks' own 2048, so the surface is drawn
 *  at the resolution the track painted it rather than at half of it, and matches the terrain
 *  grid it lies across. A track's own is rarely bigger, and past this the picture costs more
 *  to move than it adds. */
const OVERVIEW_DIM = 2048;

/** Metres. Below this, `[info] length` is a placeholder rather than a lap. */
const MIN_PLAUSIBLE_LENGTH = 50;

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
  const [info, setInfo] = useState<TrackInfo | null>(null);
  const [terrain, setTerrain] = useState<TrackTerrain | null>(null);
  const [overview, setOverview] = useState<TrackOverview | null>(null);
  const [scenery, setScenery] = useState<TrackScenery | null>(null);
  const [surfaces, setSurfaces] = useState<TrackSceneryTexture[]>([]);
  const [backdrop, setBackdrop] = useState<TrackBackdrop | null>(null);
  const [ground, setGround] = useState<TrackSceneryTexture | null>(null);
  const [placements, setPlacements] = useState<TrackPlacement[]>([]);
  // On by default: the scenery is the difference between a shape and a place, and a track
  // that carries none simply has nothing to switch off.
  const [showObjects, setShowObjects] = useState(true);
  // True only until the *coarse* pass lands — the refine that follows happens under a
  // terrain that is already up, and covering it with a spinner would be a step backwards.
  const [loading, setLoading] = useState(false);
  const [refining, setRefining] = useState(false);
  // The surfaces are still inflating; the scenery is up but grey.
  const [painting, setPainting] = useState(false);
  // What the last click on the scenery landed on, so the panel can name it.
  const [picked, setPicked] = useState<PickedPiece | null>(null);
  // A scenery pass that fails has to say so. Swallowing it leaves a track looking as though
  // it simply has none, which is a different fact entirely.
  const [sceneryError, setSceneryError] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  // Only fetched when a track fails, and only when asked for — it re-reads the archive.
  const [diagnosis, setDiagnosis] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    if (!open) return;
    let alive = true;

    setInfo(null);
    setTerrain(null);
    setOverview(null);
    setScenery(null);
    setSurfaces([]);
    setBackdrop(null);
    setGround(null);
    setPicked(null);
    setSceneryError(null);
    setPlacements([]);
    setError(null);
    setDiagnosis(null);
    setCopied(false);
    setLoading(true);

    // The inventory doesn't inflate anything, so it lands while the terrain is still being
    // read and the panel can fill in around the empty canvas.
    readTrackInfo(path)
      .then((i) => alive && setInfo(i))
      .catch(() => {});

    // Kilobytes of cfg, so these land while the terrain is still being read — the markers
    // are up before the scenery they stand among.
    readTrackPlacements(path)
      .then((p) => alive && setPlacements(p))
      .catch(() => {});

    // The sky is a few hundred triangles, so it lands with the first pass rather than after
    // the scenery — a track should never be on screen with nothing above it.
    loadTrackBackdrop(path)
      .then((b) => alive && setBackdrop(b))
      .catch(() => {});

    // One small sheet, so it lands early and the ground has grain from the first frame the
    // terrain is up.
    loadTrackGround(path)
      .then((g) => alive && setGround(g))
      .catch(() => {});

    void (async () => {
      try {
        // Started before anything is awaited, so the archive read it needs overlaps the
        // terrain's rather than following it.
        const surface = loadTrackOverview(path, OVERVIEW_DIM).catch(() => null);
        // The heaviest read in the view — most of a track's bulk is its `.map` — so it is
        // started here and settled last, under a terrain that is already up.
        const objects = loadTrackScenery(path).catch((e) => {
          setSceneryError(e instanceof Error ? e.message : String(e));
          return null;
        });

        const coarse = await loadTrackTerrain(path, COARSE_DIM);
        if (!alive) return;
        setTerrain(coarse);
        setLoading(false);

        setRefining(true);
        // Both together, and applied in one go: the mesh is built from the terrain *and*
        // whether there's a surface to lay on it, so settling them separately would build a
        // grid of a million-odd vertices twice and throw the first away.
        const [fine, map] = await Promise.all([loadTrackTerrain(path, FINE_DIM), surface]);
        if (!alive) return;
        setOverview(map);
        // Settled on its own: the scenery is a separate mesh, so it can arrive after the
        // terrain has refined without either waiting on the other. Its surfaces are asked
        // for only once the mesh is up, so the track is on screen — in outline — while the
        // hundreds of megabytes behind its colours are still inflating.
        void objects.then((s) => {
          if (!alive) return;
          setScenery(s);
          if (!s) return;
          setPainting(true);
          loadTrackSurfaces(path)
            .then((t) => alive && setSurfaces(t))
            .catch((e) =>
              setSceneryError(e instanceof Error ? e.message : String(e)),
            )
            .finally(() => alive && setPainting(false));
        });
        // Only an upgrade: a backend that capped the master below the fine size would
        // otherwise have us swap a grid for an identical one and rebuild the mesh for free.
        if (fine.width > coarse.width) setTerrain(fine);
      } catch (e) {
        if (!alive) return;
        setError(e instanceof Error ? e.message : String(e));
      } finally {
        if (alive) {
          setLoading(false);
          setRefining(false);
        }
      }
    })();

    return () => {
      alive = false;
    };
  }, [open, path]);

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
              placements={placements}
              showObjects={showObjects}
              onPick={setPicked}
              className="absolute inset-0"
            />
            {loading && !terrain && (
              <div className="pointer-events-none absolute inset-0 flex flex-col items-center justify-center gap-3 bg-black/40">
                <Loader2 className="h-6 w-6 animate-spin text-white/80" />
                <span className="text-sm text-white/80">{t("trackViewer.loading")}</span>
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
