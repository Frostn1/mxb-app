import { useMemo } from "react";
import { TrackViewer, type ViewerLine } from "@frost/shared/Components/Viewer/TrackViewer";
import type { TrackOverview, TrackTerrain } from "@frost/shared/types";
import type { Review, Surface } from "@/api/coach";

/** Points along the drawn paths and channels: one every 2 m. */
const STEP = 2;

/** What the 3D view stands the laps on. */
export interface Ground3D {
  terrain: TrackTerrain;
  overview: TrackOverview | null;
  /** World x/z of the terrain grid's corner; the viewer puts it at the origin. */
  origin: [number, number];
  /** How high the bike rides above this ground, metres. */
  lift: number;
}

/**
 * The ridden ground as a terrain grid. Ground nobody rode takes the height of the nearest
 * ground somebody did, spreading out from the track, so it sits in a plain rather than on a
 * causeway with cliffs either side.
 */
export function surfaceTerrain(surface: Surface): TrackTerrain {
  const { width, height } = surface;
  const heights = new Float32Array(width * height);
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

/**
 * The ground in 3D with the fast lap in grey, this lap in blue and the picked section in its
 * colour, the camera brought to the section.
 */
export default function Track3D({
  review,
  ground,
  selected,
  className,
}: {
  review: Review;
  ground: Ground3D;
  selected: number | null;
  className?: string;
}) {
  const { paths, channels } = review;
  const sel = selected != null ? review.sections[selected] : null;
  const [ox, oz] = ground.origin;

  const lines = useMemo<ViewerLine[]>(() => {
    const at = (pts: [number, number][], ys: number[], k: number): [number, number, number] => [
      pts[k][0] - ox,
      (ys[k] ?? 0) - ground.lift + 0.3,
      pts[k][1] - oz,
    ];
    const whole = (pts: [number, number][], ys: number[]) => pts.map((_, k) => at(pts, ys, k));
    const out: ViewerLine[] = [
      { points: whole(paths.reference, channels.height.reference), colour: "#8a8a93", width: 1.5 },
      { points: whole(paths.lap, channels.height.lap), colour: "#9ccfec", width: 2 },
    ];
    if (sel) {
      const [a, b] = [Math.floor(sel.start / STEP), Math.min(paths.lap.length - 1, Math.ceil(sel.end / STEP))];
      const pts = paths.lap.slice(a, b + 1).map((_, k) => at(paths.lap, channels.height.lap, a + k));
      out.push({ points: pts, colour: loss(sel.lost), width: 5 });
    }
    return out;
  }, [paths, channels, sel, ox, oz, ground.lift]);

  const focus = useMemo(() => {
    if (!sel) return null;
    const p = paths.reference[Math.min(paths.reference.length - 1, Math.round((sel.core[0] + sel.core[1]) / 2 / STEP))];
    return { x: p[0] - ox, z: p[1] - oz };
  }, [sel, paths, ox, oz]);

  return (
    <TrackViewer terrain={ground.terrain} overview={ground.overview} lines={lines} focus={focus} className={className} />
  );
}

/** Three.js wants real colours, not CSS variables. */
function loss(lost: number): string {
  if (lost > 0.05) return "#ef8078";
  if (lost < -0.05) return "#6fd99a";
  return "#f2f3f5";
}
