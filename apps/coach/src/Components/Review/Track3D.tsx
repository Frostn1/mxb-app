import { useMemo, useState } from "react";
import { Boxes, Loader2 } from "lucide-react";
import { TrackViewer, type ViewerActor, type ViewerLine } from "@frost/shared/Components/Viewer/TrackViewer";
import { useTrackScene } from "@frost/shared/Components/Viewer/useTrackScene";
import { Button } from "@frost/shared/Components/ui/button";
import { cn } from "@frost/shared/lib/utils";
import type { TrackTerrain } from "@frost/shared/types";
import { useT } from "@/i18n";
import type { Ground, Lines, Review, Surface } from "@/api/coach";

/** How tall a tip's post stands above the ground, and how far its flag reaches, in metres. */
const POST = 3.2;
const FLAG = 1.8;

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

const YOU = "#2997ff";
const REF = "#8a8a93";

/** A swatch and a word, for the line colours over the track. */
function Key({ colour, children }: { colour: string; children: string }) {
  return (
    <span className="flex items-center gap-1.5">
      <span className="inline-block h-[3px] w-4 rounded" style={{ background: colour }} />
      {children}
    </span>
  );
}

/** Two swatches running one into the other, for a scale rather than a single line. */
function Scale({ from, to, children }: { from: string; to: string; children: string }) {
  return (
    <span className="flex items-center gap-1.5">
      <span
        className="inline-block h-[3px] w-6 rounded"
        style={{ background: `linear-gradient(to right, ${from}, ${to})` }}
      />
      {children}
    </span>
  );
}

/**
 * The track in 3D as MXB App shows it, when its files can be read: terrain, scenery and the
 * ground in game view, with this lap, the fast lap, the picked section and a post at every tip
 * drawn on it. When they can't, the ground built from the laps, and why.
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
  actor,
  follow = false,
  onGround,
  legend = true,
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
  /**
   * A bike to ride the track, for a replay.
   *
   * Its `at` is in the same space as the review's own path points — the raw sample position —
   * because that is what a caller has. The shift onto the drawn grid is this component's, for
   * the same reason the lines' is: it is the only piece that knows which ground is underneath.
   */
  actor?: ViewerActor | null;
  /** Keep the camera on the actor as it moves, instead of on the picked section. */
  follow?: boolean;
  /** A click on the ground, in the same space as the review's own path points. */
  onGround?: (at: { x: number; z: number }) => void;
  /** The colour key under the view. Off where the caller says what the colours mean itself. */
  legend?: boolean;
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
        // Every stint of the session is drawn, and each starts counting at lap 1 again: it
        // takes the stint as well to leave out the lap that's already drawn in blue.
        if (l.lap === lap && l.stint === lines.stint) continue;
        const hue = Math.round(120 * (1 - (l.time - lo) / Math.max(hi - lo, 0.01)));
        out.push({ points: whole(l.path, l.heights), colour: `hsl(${hue}, 65%, 55%)`, width: 1.2 });
      }
    }
    // The ideal lap has no line: it is a time for each section, not a lap anybody rode.
    if (review.traced) out.push({ points: whole(paths.reference, paths.referenceY), colour: REF, width: 1.5 });
    out.push({ points: whole(paths.lap, paths.lapY), colour: YOU, width: 2.2 });
    if (sel) {
      const [a, b] = [Math.floor(sel.start / step), Math.min(paths.lap.length - 1, Math.ceil(sel.end / step))];
      const pts = paths.lap.slice(a, b + 1).map((_, k) => at(paths.lap, paths.lapY, a + k));
      out.push({ points: pts, colour: loss(sel.lost), width: 5 });
    }
    // A post where each tip happens, so "out of turn 2" is somewhere you can see. The sections
    // worth working on always stand; the picked one stands taller, in its own colour.
    const marked = new Set<number>(review.focus);
    if (selected != null) marked.add(selected);
    for (const i of marked) {
      const s = review.sections[i];
      if (!s) continue;
      const colour = i === selected ? loss(s.lost) : "#f5f5f7";
      const width = i === selected ? 3 : 2;
      for (const f of s.findings) {
        if (f.skill === "unclear") continue;
        const k = Math.max(0, Math.min(paths.lap.length - 1, Math.round(f.at / step)));
        const p = paths.lap[k];
        if (!p) continue;
        const [x, z] = [p[0] - ox, p[1] - oz];
        const base = (paths.lapY[k] ?? 0) - lift + 0.3;
        const top = base + POST;
        out.push({ points: [[x, base, z], [x, top, z]], colour, width });
        out.push({ points: [[x, top, z], [x + FLAG, top - 0.6, z], [x, top - 1.1, z]], colour, width });
      }
    }
    return out;
  }, [paths, lines, allLaps, lap, review.traced, review.focus, review.sections, selected, sel, step, ox, oz, lift]);

  const focus = useMemo(() => {
    if (!sel) return null;
    // Where the camera looks: the reference lap's line, or this lap's when there is none.
    const on = paths.reference.length > 0 ? paths.reference : paths.lap;
    const p = on[Math.min(on.length - 1, Math.round((sel.core[0] + sel.core[1]) / 2 / step))];
    return { x: p[0] - ox, z: p[1] - oz };
  }, [sel, paths, step, ox, oz]);

  // The track is on its way: say so over the canvas rather than in 11px under it. The ground
  // built from the laps is drawn meanwhile and looks finished, which is exactly how a rider
  // ends up believing the blurred grid is their circuit.
  // The bike, moved onto the same grid the lines are drawn on. Not the lines' own +0.3: that
  // lifts a hairline clear of the ground it is painted on, and a bike raised by it hovers.
  const placed = useMemo<ViewerActor | null>(
    () => (actor ? { ...actor, at: [actor.at[0] - ox, actor.at[1] - lift, actor.at[2] - oz] } : null),
    [actor, ox, oz, lift],
  );

  // Following the bike and framing the corner are the same camera, so only one may drive it:
  // a focus that keeps firing would drag the view back off the bike every time it changed.
  const chase = useMemo(
    () => (follow && placed ? { x: placed.at[0], z: placed.at[2] } : null),
    [follow, placed],
  );

  const waiting = ground != null && !scene.terrain;
  // Not the track at all, and it isn't coming: it isn't in the rider's mods, it's locked, or
  // its terrain wouldn't read. Anything that reads is drawn, so this is now rare.
  const guessing = !waiting && !real;
  // Drawn, but the lines over it may sit a little off. A note, not a banner — the track is
  // there and that is what the rider came to see.
  const roughFit = real && ground?.roughFit === true;
  const note = real && scene.painting ? t("review.loadingTrack") : "";

  return (
    <div className={cn("flex flex-col", className)}>
      {/* The viewer keeps its own corner for the drag and zoom hints, so the key sits under it
          rather than on top of them. */}
      <div className="relative min-h-0 flex-1">
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
          actor={placed}
          focus={chase ? null : focus}
          follow={chase}
          // The viewer answers in the grid's frame; the caller thinks in the lap's, which is
          // the one the shift above took the lines out of.
          onGroundClick={onGround && ((at) => onGround({ x: at.x + ox, z: at.z + oz }))}
          className="h-full w-full"
        />
        {waiting && (
          <div className="pointer-events-none absolute inset-0 flex flex-col items-center justify-center gap-3 bg-black/45">
            <Loader2 className="size-6 animate-spin text-white/80" />
            <span className="text-[13px] text-white/85">{t("review.loadingTrack")}</span>
            <span className="max-w-xs text-center text-[11.5px] text-white/60">{t("review.loadingTrackHint")}</span>
          </div>
        )}
        {guessing && (
          <div className="pointer-events-none absolute inset-x-3 top-3 border border-warning/40 bg-black/70 px-3 py-2 backdrop-blur-sm">
            <div className="text-[12.5px] font-semibold text-warning">{t("review.notYourTrack")}</div>
            <div className="mt-0.5 text-[11.5px] text-white/70">
              {t("review.groundFromLaps")}
              {why ? ` ${why}.` : ""}
            </div>
          </div>
        )}
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
      </div>
      {legend && (
      <div className="flex flex-wrap items-center gap-x-4 gap-y-1 border-t border-border px-3 py-2 text-[11px] text-muted-foreground">
        {/* Every colour on the track gets a word, and only the ones actually drawn: a key for
            a line that isn't there is worse than no key. */}
        <Key colour={YOU}>{t("review.legendYou")}</Key>
        {!review.solo && review.traced && <Key colour={REF}>{t("review.legendRef")}</Key>}
        {allLaps && <Scale from="hsl(120, 65%, 55%)" to="hsl(0, 65%, 55%)">{t("review.legendLaps")}</Scale>}
        {sel && (
          <Key colour={loss(sel.lost)}>
            {t(sel.lost > 0.05 ? "review.legendLost" : sel.lost < -0.05 ? "review.legendGained" : "review.legendLevel", {
              name: sel.name,
            })}
          </Key>
        )}
        <span>{t("review.tipsOnTrack")}</span>
        {roughFit && (
          <span className="text-faint">
            {t("review.linesRough")}
            {why ? ` ${why}.` : ""}
          </span>
        )}
        {note && <span className="text-faint">{note}</span>}
      </div>
      )}
    </div>
  );
}
