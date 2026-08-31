import { useRef, useState } from "react";

import { cn } from "@/lib/utils";
import { FEATURE_COLOUR, featureSpan, type TrackFeature } from "../../../api/trackgen";

export interface Knot {
  at: number;
  height: number;
}

interface Props {
  /** Metres round the lap. */
  lap: number;
  /**
   * The stretch of lap on screen, in metres. Defaults to all of it.
   *
   * Scoping this to one corner or one straight is the difference between a curve you can
   * shape and a curve you can only wave at: on a 1500 m lap, a 40 m berm is three pixels
   * wide and every point you place lands on top of the last.
   */
  from?: number;
  to?: number;
  knots: Knot[];
  /**
   * Drawn along the bottom, and draggable there: a feature's position is the one thing about
   * it that no row can edit, and this is the natural place to say where a jump goes.
   */
  features: TrackFeature[];
  onChange: (knots: Knot[]) => void;
  /** Move or resize one feature. Called once, when the drag ends. */
  onFeature: (index: number, patch: Partial<TrackFeature>) => void;
  /** Told which feature the pointer is over, for lighting it up in the 3D view. */
  onHover?: (index: number | null) => void;
  className?: string;
}

/** How close to a bar's right edge counts as grabbing the edge rather than the bar. */
const EDGE_PX = 7;

/** The bar row's share of the strip. */
const BAR_TOP = 88;

/**
 * The curve's height at a point, easing between neighbours and wrapping round the lap.
 *
 * The same shape `apply_elevation` builds in the synthesiser and `elevationAt` reads in the
 * api — three copies of one curve, which is two more than anybody wants, but drawing it here
 * from the same rule is what stops the picture disagreeing with the ground.
 */
function heightAt(sorted: Knot[], s: number, lap: number): number {
  if (sorted.length === 0) return 0;
  if (sorted.length === 1) return sorted[0].height;
  let i = sorted.findIndex((p) => p.at > s);
  i = i <= 0 ? sorted.length - 1 : i - 1;
  const a = sorted[i];
  const b = sorted[(i + 1) % sorted.length];
  const span = b.at > a.at ? b.at - a.at : lap - a.at + b.at;
  if (span <= 1e-3) return b.height;
  const along = s >= a.at ? s - a.at : lap - a.at + s;
  const u = Math.min(Math.max(along / span, 0), 1);
  return a.height + (b.height - a.height) * (u * u * (3 - 2 * u));
}

/** Metres shown above and below zero when the curve is flat or nearly so. */
const MIN_RANGE = 6;

/** How far from a point a click counts as grabbing it rather than adding one. */
const GRAB_PX = 14;

/**
 * The track's height along the lap, as a line you can pull about.
 *
 * The same shape a run of `rise` values describes, said as a curve instead of as a list of
 * instructions — which is the form you can take hold of. Drag a point to move it, click the
 * line to add one, double-click a point to take it away.
 *
 * The jumps are drawn along the bottom in their own colours, because "put a hill under the
 * rhythm section" is the thing this is for, and it can't be done against an empty axis.
 */
export default function ElevationCurve({
  lap,
  from = 0,
  to,
  knots,
  features,
  onChange,
  onFeature,
  onHover,
  className,
}: Props) {
  const box = useRef<HTMLDivElement>(null);
  // The point being dragged, held here rather than pushed upstream on every move: a drag is
  // a hundred events, each of which would otherwise rebuild and re-measure the whole track.
  // The line follows the pointer from this; the program hears about it once, on release.
  const [drag, setDrag] = useState<{ index: number; knot: Knot } | null>(null);
  const end = to ?? lap;
  const width = Math.max(end - from, 1);
  // A feature being moved or stretched. Held here for the same reason the curve point is:
  // a drag is a hundred events and only the last is worth building.
  const [bar, setBar] = useState<{
    index: number;
    edge: boolean;
    at: number;
    size: number;
    grabbed: number;
  } | null>(null);

  const live = drag
    ? knots.map((k, i) => (i === drag.index ? drag.knot : k))
    : knots;
  const span = Math.max(MIN_RANGE, ...live.map((k) => Math.abs(k.height) + 1));
  // Sorted for drawing only. The array itself keeps its order, so dragging a point past its
  // neighbour doesn't renumber the thing under the pointer and swap which one you are moving.
  const sorted = [...live].sort((a, b) => a.at - b.at);

  const toX = (at: number) => ((at - from) / width) * 100;
  const toY = (h: number) => 50 - (h / span) * 45;

  /** Where in the lap, and at what height, a pointer is. */
  function fromPointer(e: React.PointerEvent) {
    const r = box.current?.getBoundingClientRect();
    if (!r) return null;
    const fx = Math.min(Math.max((e.clientX - r.left) / r.width, 0), 1);
    const fy = Math.min(Math.max((e.clientY - r.top) / r.height, 0), 1);
    return { at: from + fx * width, height: ((0.5 - fy) / 0.45) * span };
  }

  /** The size a bar's right edge controls, which is not `length` for every kind. */
  function sizeOf(f: TrackFeature): { key: string; value: number } {
    if (f.kind === "double") return { key: "gap", value: f.gap };
    if (f.kind === "whoops") return { key: "spacing", value: f.spacing };
    return { key: "length", value: (f as { length: number }).length };
  }

  function onDown(e: React.PointerEvent) {
    const r = box.current?.getBoundingClientRect();
    const here = fromPointer(e);
    if (!r || !here) return;

    // The bar row first: it sits over the bottom of the curve's area, and a click down there
    // is far more likely to be aimed at a jump than at the line.
    if ((e.clientY - r.top) / r.height > BAR_TOP / 100) {
      const hit = features.findIndex((f) => {
        const x0 = ((f.at - from) / width) * r.width;
        const x1 = ((f.at + featureSpan(f).length - from) / width) * r.width;
        const x = e.clientX - r.left;
        return x >= x0 - 2 && x <= x1 + 2;
      });
      if (hit >= 0) {
        const f = features[hit];
        const x1 = ((f.at + featureSpan(f).length) / lap) * r.width;
        (e.target as HTMLElement).setPointerCapture(e.pointerId);
        setBar({
          index: hit,
          edge: Math.abs(e.clientX - r.left - x1) < EDGE_PX,
          at: f.at,
          size: sizeOf(f).value,
          grabbed: here.at,
        });
        return;
      }
    }
    // Nearest point, in pixels — the two axes are on wildly different scales, so metres
    // would make a point at the far end of a long lap easier to grab than one underfoot.
    let near = -1;
    let best = GRAB_PX;
    live.forEach((k, i) => {
      const dx = (toX(k.at) / 100) * r.width - (e.clientX - r.left);
      const dy = (toY(k.height) / 100) * r.height - (e.clientY - r.top);
      const d = Math.hypot(dx, dy);
      if (d < best) {
        best = d;
        near = i;
      }
    });
    (e.target as HTMLElement).setPointerCapture(e.pointerId);
    if (near >= 0) {
      setDrag({ index: near, knot: live[near] });
      return;
    }
    // A new point, added where the line was clicked and then dragged from there.
    const next = [...knots, here];
    setDrag({ index: next.length - 1, knot: here });
    onChange(next);
  }

  function onMove(e: React.PointerEvent) {
    const here = fromPointer(e);
    if (!here) return;
    if (bar) {
      const moved = here.at - bar.grabbed;
      setBar(
        bar.edge
          ? { ...bar, size: Math.max(2, bar.size + moved), grabbed: here.at }
          : {
              ...bar,
              // Kept inside the lap while dragging rather than clamped afterwards, so the
              // bar goes where the pointer does right up to the line and then stops.
              at: Math.min(
                Math.max(0, bar.at + moved),
                Math.max(0, lap - featureSpan(features[bar.index]).length),
              ),
              grabbed: here.at,
            },
      );
      return;
    }
    if (drag) setDrag({ ...drag, knot: here });
  }

  function onUp() {
    if (bar) {
      const f = features[bar.index];
      onFeature(
        bar.index,
        (bar.edge
          ? { [sizeOf(f).key]: bar.size }
          : { at: bar.at }) as Partial<TrackFeature>,
      );
    }
    if (drag) onChange(knots.map((k, i) => (i === drag.index ? drag.knot : k)));
    setBar(null);
    setDrag(null);
  }

  return (
    <div
      ref={box}
      className={cn("relative select-none", className)}
      onPointerDown={onDown}
      onPointerMove={onMove}
      onPointerUp={onUp}
      onPointerCancel={onUp}
    >
      <svg viewBox="0 0 100 100" preserveAspectRatio="none" className="h-full w-full">
        {/* Ground level, which is what the numbers are relative to. */}
        <line x1="0" y1="50" x2="100" y2="50" className="stroke-input" strokeWidth="0.4" />

        {/* Where the jumps are, so a hill can be put under one on purpose. */}
        {features.map((f, i) => {
          const live = bar?.index === i ? bar : null;
          const at = live ? live.at : f.at;
          const length = live && live.edge ? featureSpan({ ...f, [sizeOf(f).key]: live.size } as TrackFeature).length : featureSpan(f).length;
          return (
            <rect
              key={i}
              x={toX(at)}
              y={BAR_TOP + 6}
              width={Math.max(toX(length), 0.4)}
              height={5}
              fill={FEATURE_COLOUR[f.kind]}
              opacity={bar?.index === i ? 1 : 0.75}
              onPointerEnter={() => onHover?.(i)}
              onPointerLeave={() => onHover?.(null)}
              style={{ cursor: "ew-resize" }}
            />
          );
        })}

        {sorted.length > 0 && (
          <polyline
            points={[
              // Sampled across the view rather than drawn point to point, so the line shows
              // the easing the terrain actually takes — and so a view holding no points at
              // all still shows the height it inherits from its neighbours.
              ...Array.from({ length: 65 }, (_, i) => {
                const at = from + (width * i) / 64;
                return `${(i / 64) * 100},${toY(heightAt(sorted, at, lap))}`;
              }),
            ].join(" ")}
            fill="none"
            className="stroke-primary"
            strokeWidth="0.8"
            vectorEffect="non-scaling-stroke"
          />
        )}
      </svg>

      {/* Handles as real elements rather than SVG circles: the viewBox is stretched, which
          would make them ovals. */}
      {live.map((k, i) => (
        k.at < from - 0.5 || k.at > end + 0.5 ? null : (
        <button
          key={i}
          onDoubleClick={(e) => {
            e.stopPropagation();
            onChange(knots.filter((_, j) => j !== i));
          }}
          style={{ left: `${toX(k.at)}%`, top: `${toY(k.height)}%` }}
          className={cn(
            "absolute size-2.5 -translate-x-1/2 -translate-y-1/2 cursor-default rounded-full border border-background bg-primary",
            drag?.index === i && "ring-2 ring-primary/50",
          )}
          title={`${k.at.toFixed(0)} m · ${k.height.toFixed(1)} m`}
        />
        )
      ))}
    </div>
  );
}
