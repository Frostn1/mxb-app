import { useMemo, useState } from "react";
import { Boxes } from "lucide-react";
import { TrackViewer, type ViewerLine } from "@frost/shared/Components/Viewer/TrackViewer";
import { useTrackScene } from "@frost/shared/Components/Viewer/useTrackScene";
import { Button } from "@frost/shared/Components/ui/button";
import { cn } from "@frost/shared/lib/utils";
import type { TrackTerrain } from "@frost/shared/types";
import { useT } from "@/i18n";
import type { Ground, Lines, Review, Surface } from "@/api/coach";

/**
 * The ridden ground as a terrain grid. Ground nobody rode takes the height of the nearest
 * ground somebody did, spreading out from the track, so it sits in a plain rather than on a
 * causeway; then it's smoothed, since one bike's height a metre apart reads as steps.
 */
export function surfaceTerrain(surface: Surface): TrackTerrain {
  const { width, height } = surface;
  let heights = new Float32Array(width * height);
  const known = new Uint8Array(width * height);
  const queue: number[] = [];
  surface.heights.forEach((h, k) => {
    if (h == null) return;
    heights[k] = h;
    known[k] = 1;
    queue.push(k);
  });
  for (let q = 0; q < queue.length; q++) {
    const k = queue[q];
    const [r, c] = [Math.floor(k / width), k % width];
    for (const [dr, dc] of [[1, 0], [-1, 0], [0, 1], [0, -1]]) {
      const [rr, cc] = [r + dr, c + dc];
      if (rr < 0 || cc < 0 || rr >= height || cc >= width) continue;
      const n = rr * width + cc;
      if (known[n]) continue;
      known[n] = 1;
      heights[n] = heights[k];
      queue.push(n);
    }
  }
  for (let pass = 0; pass < 3; pass++) {
    const src = heights;
    heights = new Float32Array(width * height);
    for (let r = 0; r < height; r++) {
      for (let c = 0; c < width; c++) {
        let [sum, n] = [0, 0];
        for (let dr = -1; dr <= 1; dr++) {
          for (let dc = -1; dc <= 1; dc++) {
            const [rr, cc] = [r + dr, c + dc];
            if (rr < 0 || cc < 0 || rr >= height || cc >= width) continue;
            sum += src[rr * width + cc];
            n++;
          }
        }
        heights[r * width + c] = sum / n;
      }
    }
  }
  let [lo, hi] = [Infinity, -Infinity];
  for (const h of heights) [lo, hi] = [Math.min(lo, h), Math.max(hi, h)];
  return {
    width,
    height,
    metresPerSample: surface.cell,
    minHeight: lo,
    maxHeight: hi,
    scaleKnown: true,
    confidence: 1,
    heightsInMetres: true,
    heights,
  };
}

/** Three.js wants real colours, not CSS variables. */
function loss(lost: number): string {
  if (lost > 0.05) return "#ff6961";
  if (lost < -0.05) return "#30d158";
  return "#f5f5f7";
}

/**
 * The track in 3D as MXB App shows it, when its files can be read: terrain, scenery and the
 * ground in game view, with this lap, the fast lap and the picked section drawn on it. When
 * they can't, the ground built from the laps, and why.
 */
export default function Track3D({
  review,
  ground,
  why,
  surface,
  lines,
  allLaps,
  lap,
  selected,
  className,
}: {
  review: Review;
  /** The track's own files, when they line up with the laps. */
  ground: Ground | null;
  /** Why they aren't used, when they aren't. */
  why: string | null;
  surface: Surface | null;
  lines: Lines | null;
  allLaps: boolean;
  lap: number;
  selected: number | null;
  className?: string;
}) {
  const t = useT();
  const scene = useTrackScene(ground?.path ?? null, { prefix: ground?.prefix ?? null });
  const [objects, setObjects] = useState(true);
  const fallback = useMemo(() => (surface ? surfaceTerrain(surface) : null), [surface]);
  const real = ground != null && scene.terrain != null;
  const terrain = real ? scene.terrain : fallback;
  // The viewer puts a grid's corner at the world origin: the track's own grid is already
  // there, the ridden one starts where the laps do.
  const [ox, oz] = real || !surface ? [0, 0] : [surface.x0, surface.z0];
  const lift = real && ground ? ground.lift : 0;
  const { paths } = review;
  const step = paths.step || 1;
  const sel = selected != null ? review.sections[selected] : null;

  const drawn = useMemo<ViewerLine[]>(() => {
    const at = (pts: [number, number][], ys: number[], k: number): [number, number, number] => [
      pts[k][0] - ox,
      (ys[k] ?? 0) - lift + 0.3,
      pts[k][1] - oz,
    ];
    const whole = (pts: [number, number][], ys: number[]) => pts.map((_, k) => at(pts, ys, k));
    const out: ViewerLine[] = [];
    if (allLaps && lines) {
      const times = lines.laps.map((l) => l.time);
      const [lo, hi] = [Math.min(...times), Math.max(...times)];
      for (const l of lines.laps) {
        if (l.lap === lap) continue;
        const hue = Math.round(120 * (1 - (l.time - lo) / Math.max(hi - lo, 0.01)));
        out.push({ points: whole(l.path, l.heights), colour: `hsl(${hue}, 65%, 55%)`, width: 1.2 });
      }
    }
    // The ideal lap has no line: it is a time for each section, not a lap anybody rode.
    if (review.traced) out.push({ points: whole(paths.reference, paths.referenceY), colour: "#8a8a93", width: 1.5 });
    out.push({ points: whole(paths.lap, paths.lapY), colour: "#2997ff", width: 2.2 });
    if (sel) {
      const [a, b] = [Math.floor(sel.start / step), Math.min(paths.lap.length - 1, Math.ceil(sel.end / step))];
      const pts = paths.lap.slice(a, b + 1).map((_, k) => at(paths.lap, paths.lapY, a + k));
      out.push({ points: pts, colour: loss(sel.lost), width: 5 });
    }
    return out;
  }, [paths, lines, allLaps, lap, review.traced, sel, step, ox, oz, lift]);

  const focus = useMemo(() => {
    if (!sel) return null;
    // Where the camera looks: the reference lap's line, or this lap's when there is none.
    const on = paths.reference.length > 0 ? paths.reference : paths.lap;
    const p = on[Math.min(on.length - 1, Math.round((sel.core[0] + sel.core[1]) / 2 / step))];
    return { x: p[0] - ox, z: p[1] - oz };
  }, [sel, paths, step, ox, oz]);

  return (
    <div className={cn("relative", className)}>
      <TrackViewer
        terrain={terrain}
        overview={real ? scene.overview : null}
        scenery={real ? scene.scenery : null}
        surfaces={real ? scene.surfaces : []}
        backdrop={real ? scene.backdrop : null}
        ground={real ? scene.ground : null}
        groundLayers={real ? scene.groundLayers : []}
        placements={real ? scene.placements : []}
        showObjects={objects}
        // The game's own look, the way MXB App shows a track.
        gameView={real && scene.groundLayers.length > 0}
        lines={drawn}
        focus={focus}
        className="h-full w-full"
      />
      {real && (scene.scenery || scene.placements.length > 0) && (
        <Button
          size="sm"
          variant={objects ? "outline" : "ghost"}
          className="absolute left-3 top-3 h-7 gap-1.5 px-2 text-[12px]"
          aria-pressed={objects}
          onClick={() => setObjects((v) => !v)}
        >
          <Boxes className="size-3.5" />
          {t("review.objects")}
        </Button>
      )}
      <p className="pointer-events-none absolute left-3 top-3 max-w-[70%] text-[11px] text-muted-foreground">
        {ground && !scene.terrain
          ? t("review.loadingTrack")
          : !real
            ? `${t("review.groundFromLaps")}${why ? ` ${why}.` : ""}`
            : scene.painting
              ? t("review.loadingTrack")
              : ""}
      </p>
    </div>
  );
}
