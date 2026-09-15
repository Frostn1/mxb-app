import { useMemo } from "react";
import { TrackViewer, type ViewerLine } from "@frost/shared/Components/Viewer/TrackViewer";
import type { TrackTerrain } from "@frost/shared/types";
import type { Review, Surface } from "@/api/coach";

/** Points along the drawn paths and channels: one every 2 m. */
const STEP = 2;

/**
 * The ridden ground in 3D, with the fast lap in grey, this lap in blue and the picked section
 * in its colour. The viewer puts the grid's corner at the world origin, so everything is
 * moved by the surface's own corner to match.
 */
export default function Track3D({
  review,
  surface,
  selected,
  className,
}: {
  review: Review;
  surface: Surface;
  selected: number | null;
  className?: string;
}) {
  const terrain = useMemo<TrackTerrain>(() => {
    const { width, height } = surface;
    // Ground nobody rode takes the height of the nearest ground somebody did, spreading out
    // from the track, so it sits in a plain rather than on a causeway with cliffs either side.
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
  }, [surface]);

  const { paths, channels } = review;
  const sel = selected != null ? review.sections[selected] : null;
  const lines = useMemo<ViewerLine[]>(() => {
    const at = (pts: [number, number][], ys: number[], k: number): [number, number, number] => [
      pts[k][0] - surface.x0,
      (ys[k] ?? 0) + 0.3,
      pts[k][1] - surface.z0,
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
  }, [paths, channels, sel, surface]);

  const focus = useMemo(() => {
    if (!sel) return null;
    const p = paths.reference[Math.min(paths.reference.length - 1, Math.round((sel.core[0] + sel.core[1]) / 2 / STEP))];
    return { x: p[0] - surface.x0, z: p[1] - surface.z0 };
  }, [sel, paths, surface]);

  return <TrackViewer terrain={terrain} lines={lines} focus={focus} className={className} />;
}

/** Three.js wants real colours, not CSS variables. */
function loss(lost: number): string {
  if (lost > 0.05) return "#ef8078";
  if (lost < -0.05) return "#6fd99a";
  return "#f2f3f5";
}
