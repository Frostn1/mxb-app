import { useEffect, useState } from "react";
import { Check, Copy, Loader2, Mountain, X } from "lucide-react";
import { Dialog, DialogClose, DialogContent } from "../ui/dialog";
import { Button } from "@/Components/ui/button";
import { TrackViewer } from "./TrackViewer";
import {
  diagnoseTrack,
  loadTrackOverview,
  loadTrackTerrain,
  readTrackInfo,
} from "../../api/tracks";
import type { TrackInfo, TrackOverview, TrackTerrain } from "../../types";
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
const FINE_DIM = 768;

/** The surface picture's longest edge. A track's own is rarely bigger, and past this the
 *  picture costs more to move than it adds to a terrain drawn at 320 samples across. */
const OVERVIEW_DIM = 1024;

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
  // True only until the *coarse* pass lands — the refine that follows happens under a
  // terrain that is already up, and covering it with a spinner would be a step backwards.
  const [loading, setLoading] = useState(false);
  const [refining, setRefining] = useState(false);
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
    setError(null);
    setDiagnosis(null);
    setCopied(false);
    setLoading(true);

    // The inventory doesn't inflate anything, so it lands while the terrain is still being
    // read and the panel can fill in around the empty canvas.
    readTrackInfo(path)
      .then((i) => alive && setInfo(i))
      .catch(() => {});

    void (async () => {
      try {
        const coarse = await loadTrackTerrain(path, COARSE_DIM);
        if (!alive) return;
        setTerrain(coarse);
        setLoading(false);

        // Fetched once the terrain is up rather than alongside it: the track is on screen
        // sooner, and a surface that never arrives leaves a working view rather than no view.
        void loadTrackOverview(path, OVERVIEW_DIM)
          .then((map) => alive && setOverview(map))
          .catch(() => {});

        setRefining(true);
        const fine = await loadTrackTerrain(path, FINE_DIM);
        if (!alive) return;
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
            {refining && (
              <span className="flex-none text-[11px] font-normal text-muted-foreground">
                {t("trackViewer.refining")}
              </span>
            )}
          </div>
          <div className="flex flex-none items-center gap-2">
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
