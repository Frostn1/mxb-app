import { useEffect, useMemo, useRef, useState } from "react";
import type { Review } from "@/api/coach";
import { useT } from "@/i18n";
import { gap, lossColor } from "@/lib/format";
import { reliefImage, type Relief } from "@/lib/relief";


/** How long the frame takes to travel to a new corner. */
const MOVE_MS = 450;

/**
 * The frame, easing from wherever it is to where it should be.
 *
 * A `viewBox` is an attribute and not a CSS property, so no stylesheet will animate it — and
 * cutting straight there leaves a rider wondering whether the track itself changed. Moving it
 * by hand is what lets the eye follow the map to the corner instead of re-finding it.
 */
function useFrame(to: [number, number, number, number]): string {
  const [box, setBox] = useState(to);
  const at = useRef(to);
  const frame = useRef<number | null>(null);
  useEffect(() => {
    const from = at.current;
    if (from.every((v, i) => Math.abs(v - to[i]) < 0.01)) return;
    const started = performance.now();
    const step = (now: number) => {
      const k = Math.min(1, (now - started) / MOVE_MS);
      // Ease out: it leaves quickly and settles, which reads as the map travelling rather than
      // sliding at a constant speed like a carousel.
      const e = 1 - (1 - k) ** 3;
      const next = from.map((v, i) => v + (to[i] - v) * e) as [number, number, number, number];
      at.current = next;
      setBox(next);
      frame.current = k < 1 ? requestAnimationFrame(step) : null;
    };
    frame.current = requestAnimationFrame(step);
    return () => {
      if (frame.current != null) cancelAnimationFrame(frame.current);
      frame.current = null;
    };
  }, [to]);
  return box.join(" ");
}

export function shortName(name: string): string {
  const m = /^(Turn|Jump|Rhythm|Whoops) (\d+)$/.exec(name);
  return m ? `${m[1][0]}${m[2]}` : "";
}

/**
 * The track from above: the reference line in grey, this lap over it coloured by where it
 * lost (red) or gained (green) time. The picked section glows and the rest fade back.
 */
export default function TrackMap({
  review,
  surface,
  others,
  selected,
  cursor,
  onPick,
  solo = false,
  legend = true,
}: {
  review: Review;
  /** Reviewed on its own: no time against a fast lap to put on the label. */
  solo?: boolean;
  /** The ground, drawn under the lines when there is one: the track's own or the ridden one. */
  surface?: Relief | null;
  /** Other laps' lines, drawn thin under this one. */
  others?: { path: [number, number][]; colour: string; width?: number }[];
  selected: number | null;
  /** Metres into the lap, or null. */
  cursor: number | null;
  onPick: (i: number) => void;
  /** The key under the map. Off where the caller says what the colours mean itself. */
  legend?: boolean;
}) {
  const t = useT();
  const { lap } = review.paths;
  // The grey line under this one is the reference lap's. The ideal lap has none — it is a time
  // for each section, not a lap anybody rode — so nothing is drawn under this one and the
  // section labels sit on its own line instead.
  const ghost = review.paths.reference;
  const reference = ghost.length > 0 ? ghost : lap;
  const PATH_STEP = review.paths.step || 1;
  const [hover, setHover] = useState<number | null>(null);

  const whole = useMemo(() => {
    const pts = [...lap, ...reference];
    const xs = pts.map((p) => p[0]);
    const ys = pts.map((p) => -p[1]);
    const [x0, x1, y0, y1] = [Math.min(...xs), Math.max(...xs), Math.min(...ys), Math.max(...ys)];
    const size = Math.max(x1 - x0, y1 - y0, 1);
    const pad = size * 0.08;
    return {
      box: [x0 - pad, y0 - pad, x1 - x0 + 2 * pad, y1 - y0 + 2 * pad] as [number, number, number, number],
      size,
      centre: [(x0 + x1) / 2, (y0 + y1) / 2] as [number, number],
    };
  }, [lap, reference]);

  const clamp = (pts: [number, number][], m: number) =>
    pts[Math.min(pts.length - 1, Math.max(0, Math.round(m / PATH_STEP)))];
  const line = (pts: [number, number][]) => pts.map((p) => `${p[0]},${-p[1]}`).join(" ");
  const slice = (a: number, b: number) => lap.slice(Math.floor(a / PATH_STEP), Math.ceil(b / PATH_STEP) + 1);
  const sel = selected != null ? review.sections[selected] : null;

  /**
   * What the map is showing: the whole track, or the picked corner brought up close.
   *
   * A corner that sits at the edge of a track drawn whole is a few pixels of line, and asking
   * a rider to find it there is asking them to take the label's word for it. Framing it leaves
   * enough track around it to still say where on the lap it is — a corner with nothing round it
   * is no more use than a corner too small to see.
   */
  const view = useMemo(() => {
    if (!sel) return whole;
    const on = lap.slice(Math.floor(sel.start / PATH_STEP), Math.ceil(sel.end / PATH_STEP) + 1);
    if (on.length === 0) return whole;
    const xs = on.map((p) => p[0]);
    const ys = on.map((p) => -p[1]);
    const [x0, x1, y0, y1] = [Math.min(...xs), Math.max(...xs), Math.min(...ys), Math.max(...ys)];
    // Never closer than a third of the track: past that the surrounding line runs out of the
    // frame and the corner stops being anywhere.
    const size = Math.max(x1 - x0, y1 - y0, whole.size / 3);
    const [cx, cy] = [(x0 + x1) / 2, (y0 + y1) / 2];
    const half = size * 0.72;
    return {
      box: [cx - half, cy - half, half * 2, half * 2] as [number, number, number, number],
      size,
      centre: [cx, cy] as [number, number],
    };
  }, [sel, lap, PATH_STEP, whole]);

  const box = useFrame(view.box);
  const dot = cursor != null ? clamp(lap, cursor) : null;
  const fs = view.size / 30;
  const relief = useMemo(() => (surface ? reliefImage(surface) : ""), [surface]);

  return (
    <div className="flex h-full min-h-0 w-full flex-col">
    <svg viewBox={box} className="min-h-0 w-full flex-1" preserveAspectRatio="xMidYMid meet">
      {surface && relief && (
        <image
          href={relief}
          x={surface.x0}
          y={-(surface.z0 + surface.height * surface.cell)}
          width={surface.width * surface.cell}
          height={surface.height * surface.cell}
          preserveAspectRatio="none"
          opacity={0.85}
        />
      )}
      {ghost.length > 0 && (
        <polyline
          points={line(ghost)}
          fill="none"
          stroke="var(--faint)"
          strokeWidth={10}
          strokeOpacity={0.3}
          strokeLinejoin="round"
          strokeLinecap="round"
          vectorEffect="non-scaling-stroke"
        />
      )}
      {others?.map((o, i) => (
        <polyline
          key={`o${i}`}
          points={line(o.path)}
          fill="none"
          stroke={o.colour}
          strokeWidth={o.width ?? 1.2}
          strokeOpacity={0.85}
          strokeLinejoin="round"
          vectorEffect="non-scaling-stroke"
          className="pointer-events-none"
        />
      ))}
      {review.sections.map((s, i) => {
        const on = selected === i;
        const faded = sel != null && !on && hover !== i;
        return (
          <polyline
            key={i}
            points={line(slice(s.start, s.end))}
            fill="none"
            stroke={lossColor(s.lost)}
            strokeOpacity={faded ? 0.25 : 1}
            strokeWidth={hover === i && !on ? 4.5 : 3}
            strokeLinejoin="round"
            strokeLinecap="round"
            vectorEffect="non-scaling-stroke"
            className="cursor-pointer transition-[stroke-opacity]"
            onClick={() => onPick(i)}
            onMouseEnter={() => setHover(i)}
            onMouseLeave={() => setHover(null)}
          >
            <title>{`${s.name} ${gap(s.lost)} s`}</title>
          </polyline>
        );
      })}

      {sel && (
        <g className="pointer-events-none">
          {/* A wide soft glow under a solid line, so the section reads at a glance. */}
          <polyline
            points={line(slice(sel.start, sel.end))}
            fill="none"
            stroke={lossColor(sel.lost)}
            strokeOpacity={0.3}
            strokeWidth={22}
            strokeLinejoin="round"
            strokeLinecap="round"
            vectorEffect="non-scaling-stroke"
          />
          <polyline
            points={line(slice(sel.start, sel.end))}
            fill="none"
            stroke="var(--foreground)"
            strokeWidth={7}
            strokeLinejoin="round"
            strokeLinecap="round"
            vectorEffect="non-scaling-stroke"
          />
          <polyline
            points={line(slice(sel.start, sel.end))}
            fill="none"
            stroke={lossColor(sel.lost)}
            strokeWidth={4}
            strokeLinejoin="round"
            strokeLinecap="round"
            vectorEffect="non-scaling-stroke"
          />
          {[sel.start, sel.end].map((m, k) => {
            const p = clamp(lap, m);
            return <circle key={k} cx={p[0]} cy={-p[1]} r={fs / 4} fill="var(--foreground)" stroke="var(--background)" strokeWidth={fs / 10} />;
          })}
          <Pill at={clamp(reference, (sel.core[0] + sel.core[1]) / 2)} centre={view.centre} fs={fs} color={lossColor(sel.lost)}>
            {solo ? sel.name : `${sel.name}  ${gap(sel.lost)} s`}
          </Pill>
          {/* Each tip where it happens, numbered as in the tips and on the charts. */}
          {(() => {
            const tips = sel.findings.filter((f) => f.skill !== "unclear");
            return tips.map((f, k) => {
              // Tips a few metres apart would sit on each other: stack them up the screen.
              const stack = tips.slice(0, k).filter((o) => Math.abs(o.at - f.at) < 8).length;
              const p = clamp(lap, f.at);
              const y = -p[1] - stack * fs * 1.25;
              return (
                <g key={`f${k}`} className="select-none">
                  <circle cx={p[0]} cy={y} r={fs * 0.55} fill="var(--foreground)" stroke="var(--background)" strokeWidth={fs / 10} />
                  <text
                    x={p[0]}
                    y={y}
                    fontSize={fs * 0.7}
                    textAnchor="middle"
                    dominantBaseline="central"
                    fill="var(--background)"
                    className="font-semibold"
                  >
                    {k + 1}
                  </text>
                </g>
              );
            });
          })()}
        </g>
      )}

      {review.sections.map((s, i) => {
        const label = shortName(s.name);
        if (!label || selected === i) return null;
        const p = clamp(reference, (s.core[0] + s.core[1]) / 2);
        return (
          <text
            key={`l${i}`}
            x={p[0]}
            y={-p[1]}
            fontSize={fs * 0.8}
            textAnchor="middle"
            dominantBaseline="central"
            fill="var(--muted-foreground)"
            fillOpacity={sel ? 0.5 : 1}
            stroke="var(--background)"
            strokeWidth={fs / 6}
            paintOrder="stroke"
            className="cursor-pointer select-none font-semibold"
            onClick={() => onPick(i)}
          >
            {label}
          </text>
        );
      })}

      {dot && (
        <circle cx={dot[0]} cy={-dot[1]} r={fs / 3} fill="var(--primary)" stroke="var(--background)" strokeWidth={fs / 10} />
      )}
    </svg>
    {/* Two lines and no key is a rider guessing which one is theirs. */}
    {legend && (
      <div className="flex shrink-0 flex-wrap items-center gap-x-4 gap-y-1 pt-2 text-[11px] text-muted-foreground">
        {!solo && <Key colour="var(--faint)" wide>{t("review.legendRef")}</Key>}
        <Key colour="var(--destructive)">{t("review.legendMapLost")}</Key>
        <Key colour="var(--success)">{t("review.legendMapGained")}</Key>
        {sel && <Key colour="var(--foreground)">{t("review.legendMapPicked", { name: sel.name })}</Key>}
      </div>
    )}
    </div>
  );
}

/** A swatch and a word, for the lines over the map. */
function Key({ colour, wide, children }: { colour: string; wide?: boolean; children: string }) {
  return (
    <span className="flex items-center gap-1.5">
      <span
        className={wide ? "inline-block h-[6px] w-4 rounded" : "inline-block h-[3px] w-4 rounded"}
        style={{ background: colour, opacity: wide ? 0.5 : 1 }}
      />
      {children}
    </span>
  );
}

/** A label on a rounded plate, set off from the point it names towards the middle of the map,
 *  so a section on the edge doesn't push its label out of view. */
function Pill({
  at,
  centre,
  fs,
  color,
  children,
}: {
  at: [number, number];
  centre: [number, number];
  fs: number;
  color: string;
  children: string;
}) {
  const w = children.length * fs * 0.58 + fs * 1.2;
  const h = fs * 1.7;
  const [px, py] = [at[0], -at[1]];
  const [dx, dy] = [centre[0] - px, centre[1] - py];
  const len = Math.hypot(dx, dy) || 1;
  const reach = fs * 3;
  const [x, y] = [px + (dx / len) * reach, py + (dy / len) * reach];
  return (
    <g>
      <line x1={px} y1={py} x2={x} y2={y} stroke="var(--foreground)" strokeWidth={fs / 12} />
      <rect x={x - w / 2} y={y - h / 2} width={w} height={h} rx={h / 2} fill="var(--popover)" stroke={color} strokeWidth={fs / 10} />
      <text x={x} y={y} fontSize={fs} textAnchor="middle" dominantBaseline="central" fill="var(--foreground)" className="font-semibold">
        {children}
      </text>
    </g>
  );
}
