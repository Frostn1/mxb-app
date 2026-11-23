import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { cn } from "@frost/shared/lib/utils";
import type { Ride } from "@/api/cinema";
import { useT } from "@/i18n";
import { clock, rideLength } from "@/lib/ride";

const AIR = "rgba(255, 159, 10, 0.32)";
const AIR_TICK = "#ff9f0a";
const SPEED = "#2997ff";
const PICK = "rgba(255, 159, 10, 0.18)";
const PICK_EDGE = "#ff9f0a";
const HEAD = "#f5f5f7";
const CRASH = "rgba(255, 105, 97, 0.35)";

/** Zoom steps for the detail strip, seconds across. */
export const ZOOMS = [6, 15, 40, 120] as const;

/** Draw on a canvas at the screen's own pixel density, sized by CSS. */
function useCanvas(draw: (ctx: CanvasRenderingContext2D, w: number, h: number) => void) {
  const ref = useRef<HTMLCanvasElement>(null);
  const [size, setSize] = useState<[number, number]>([0, 0]);
  useEffect(() => {
    const c = ref.current;
    if (!c) return;
    const ro = new ResizeObserver(() => setSize([c.clientWidth, c.clientHeight]));
    ro.observe(c);
    setSize([c.clientWidth, c.clientHeight]);
    return () => ro.disconnect();
  }, []);
  useEffect(() => {
    const c = ref.current;
    if (!c || size[0] === 0) return;
    const dpr = window.devicePixelRatio || 1;
    c.width = Math.round(size[0] * dpr);
    c.height = Math.round(size[1] * dpr);
    const ctx = c.getContext("2d");
    if (!ctx) return;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, size[0], size[1]);
    draw(ctx, size[0], size[1]);
  });
  return ref;
}

/** Runs of a flag channel, as [start, end] seconds. */
function runs(r: Ride, ch: Float32Array, from: number, to: number): [number, number][] {
  const out: [number, number][] = [];
  let s: number | null = null;
  for (let k = 0; k < r.count; k++) {
    const tt = r.t[k];
    if (tt < from - 1 || tt > to + 1) {
      if (s != null) {
        out.push([s, r.t[k - 1]]);
        s = null;
      }
      continue;
    }
    const on = ch[k] > 0.5;
    if (on && s == null) s = tt;
    if (!on && s != null) {
      out.push([s, tt]);
      s = null;
    }
  }
  if (s != null) out.push([s, r.t[r.count - 1]]);
  return out;
}

/** The speed line over [from, to], one point per pixel column. */
function drawSpeed(ctx: CanvasRenderingContext2D, r: Ride, from: number, to: number, w: number, top: number, h: number, peak: number) {
  ctx.beginPath();
  let k = 0;
  for (let px = 0; px <= w; px++) {
    const tt = from + ((to - from) * px) / w;
    while (k < r.count - 1 && r.t[k + 1] <= tt) k++;
    const v = r.speed[k];
    const y = top + h - (v / peak) * h;
    if (px === 0) ctx.moveTo(px, y);
    else ctx.lineTo(px, y);
  }
  ctx.strokeStyle = SPEED;
  ctx.lineWidth = 1.5;
  ctx.stroke();
}

/**
 * The ride along time: an overview of the whole of it on top, and a zoomed strip below where
 * the moment is found and marked. Speed is the line; every stretch with both wheels off the
 * ground is shaded, so the jumps stand out before anything is played. Drag on the strip to
 * scrub; Shift-drag to mark a range.
 */
export default function Timeline({
  ride,
  at,
  span,
  range,
  onSeek,
  onRange,
  className,
}: {
  ride: Ride;
  at: number;
  /** Seconds across the detail strip. */
  span: number;
  range: [number, number] | null;
  onSeek: (t: number) => void;
  onRange: (r: [number, number] | null) => void;
  className?: string;
}) {
  const t = useT();
  const length = rideLength(ride);
  const peak = useMemo(() => {
    let p = 10;
    for (let k = 0; k < ride.count; k++) p = Math.max(p, ride.speed[k]);
    return p;
  }, [ride]);
  // The detail window turns a page when the playhead leaves it, rather than sliding under the
  // pointer while it scrubs.
  const drag = useRef<{ start: number; shift: boolean } | null>(null);
  const [win, setWin] = useState(0);
  const place = (tt: number) => Math.max(0, Math.min(Math.max(0, length - span), tt - span * 0.25));
  useEffect(() => {
    if (drag.current) return;
    if (at < win || at > win + span * 0.92) setWin(place(at));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [at, span, length]);
  useEffect(() => setWin(place(at)), [span]); // eslint-disable-line react-hooks/exhaustive-deps
  const from = Math.min(win, Math.max(0, length - span));
  const to = Math.min(length, from + span);

  const overview = useCanvas((ctx, w, h) => {
    const x = (tt: number) => (tt / Math.max(length, 0.001)) * w;
    for (const [a, b] of runs(ride, ride.air, 0, length)) {
      ctx.fillStyle = AIR_TICK;
      ctx.fillRect(x(a), h - 6, Math.max(1, x(b) - x(a)), 6);
    }
    drawSpeed(ctx, ride, 0, length, w, 3, h - 12, peak);
    ctx.fillStyle = "rgba(255,255,255,0.08)";
    ctx.fillRect(x(from), 0, Math.max(2, x(to) - x(from)), h);
    ctx.strokeStyle = "rgba(255,255,255,0.35)";
    ctx.strokeRect(x(from) + 0.5, 0.5, Math.max(2, x(to) - x(from)) - 1, h - 1);
    if (range) {
      ctx.fillStyle = PICK_EDGE;
      ctx.fillRect(x(range[0]), 0, Math.max(2, x(range[1]) - x(range[0])), 3);
    }
    ctx.fillStyle = HEAD;
    ctx.fillRect(x(at) - 0.5, 0, 1.5, h);
  });

  const detail = useCanvas((ctx, w, h) => {
    const x = (tt: number) => ((tt - from) / Math.max(to - from, 0.001)) * w;
    const top = 16;
    const body = h - top - 4;
    for (const [a, b] of runs(ride, ride.crashed, from, to)) {
      ctx.fillStyle = CRASH;
      ctx.fillRect(x(a), top, x(b) - x(a), body);
    }
    for (const [a, b] of runs(ride, ride.air, from, to)) {
      ctx.fillStyle = AIR;
      ctx.fillRect(x(a), top, Math.max(1, x(b) - x(a)), body);
    }
    // seconds ruler
    const step = span <= 8 ? 1 : span <= 20 ? 2 : span <= 60 ? 5 : 15;
    ctx.font = "10px ui-monospace, monospace";
    ctx.fillStyle = "rgba(255,255,255,0.45)";
    ctx.strokeStyle = "rgba(255,255,255,0.1)";
    for (let s = Math.ceil(from / step) * step; s <= to; s += step) {
      const px = x(s);
      ctx.beginPath();
      ctx.moveTo(px + 0.5, top);
      ctx.lineTo(px + 0.5, h);
      ctx.stroke();
      ctx.fillText(clock(s).replace(/\.\d+$/, ""), px + 3, 11);
    }
    drawSpeed(ctx, ride, from, to, w, top + 4, body - 8, peak);
    // jumps: airtime over each
    ctx.fillStyle = AIR_TICK;
    for (const j of ride.jumps) {
      if (j.apex < from || j.apex > to) continue;
      ctx.fillText(`${(j.end - j.start).toFixed(1)}s`, x(j.apex) - 10, top + 12);
    }
    if (range) {
      const a = x(range[0]);
      const b = x(range[1]);
      ctx.fillStyle = PICK;
      ctx.fillRect(a, top, Math.max(1, b - a), body);
      ctx.fillStyle = PICK_EDGE;
      ctx.fillRect(a - 1, top, 2, body);
      ctx.fillRect(b - 1, top, 2, body);
    }
    ctx.fillStyle = HEAD;
    ctx.fillRect(x(at) - 1, 0, 2, h);
  });

  const timeAt = useCallback(
    (e: React.PointerEvent<HTMLCanvasElement>, a: number, b: number) => {
      const rect = e.currentTarget.getBoundingClientRect();
      const f = Math.max(0, Math.min(1, (e.clientX - rect.left) / rect.width));
      return a + (b - a) * f;
    },
    [],
  );

  return (
    <div className={cn("space-y-1.5", className)}>
      <canvas
        ref={overview}
        className="h-9 w-full cursor-pointer rounded bg-black/30"
        aria-label={t("timeline.overview")}
        onPointerDown={(e) => onSeek(timeAt(e, 0, length))}
      />
      <canvas
        ref={detail}
        className="h-[92px] w-full cursor-ew-resize rounded bg-black/30"
        aria-label={t("timeline.detail")}
        onPointerDown={(e) => {
          e.currentTarget.setPointerCapture(e.pointerId);
          const tt = timeAt(e, from, to);
          drag.current = { start: tt, shift: e.shiftKey };
          if (e.shiftKey) onRange([tt, tt]);
          else onSeek(tt);
        }}
        onPointerMove={(e) => {
          const d = drag.current;
          if (!d) return;
          const tt = timeAt(e, from, to);
          if (d.shift) onRange([Math.min(d.start, tt), Math.max(d.start, tt)]);
          else onSeek(tt);
        }}
        onPointerUp={() => {
          drag.current = null;
        }}
      />
    </div>
  );
}
