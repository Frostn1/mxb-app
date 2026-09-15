import { useMemo } from "react";
import type { Review } from "@/api/coach";
import { lossColor } from "@/lib/format";

/** Points along the drawn paths: `paths` holds one every 2 m. */
const PATH_STEP = 2;

function short(name: string): string {
  const m = /^(Turn|Jump|Rhythm|Whoops) (\d+)$/.exec(name);
  return m ? `${m[1][0]}${m[2]}` : "";
}

/**
 * The track from above: the reference line in grey, this lap over it coloured by where it
 * lost (red) or gained (green) time. Click a section to pick it.
 */
export default function TrackMap({
  review,
  selected,
  cursor,
  onPick,
}: {
  review: Review;
  selected: number | null;
  /** Metres into the lap, or null. */
  cursor: number | null;
  onPick: (i: number) => void;
}) {
  const { lap, reference } = review.paths;

  const view = useMemo(() => {
    const pts = [...lap, ...reference];
    const xs = pts.map((p) => p[0]);
    const ys = pts.map((p) => -p[1]);
    const [x0, x1, y0, y1] = [Math.min(...xs), Math.max(...xs), Math.min(...ys), Math.max(...ys)];
    const size = Math.max(x1 - x0, y1 - y0, 1);
    const pad = size * 0.06;
    return { box: `${x0 - pad} ${y0 - pad} ${x1 - x0 + 2 * pad} ${y1 - y0 + 2 * pad}`, size };
  }, [lap, reference]);

  const line = (pts: [number, number][]) => pts.map((p) => `${p[0]},${-p[1]}`).join(" ");
  const at = (m: number) => lap[Math.min(lap.length - 1, Math.max(0, Math.round(m / PATH_STEP)))];
  const refAt = (m: number) => reference[Math.min(reference.length - 1, Math.max(0, Math.round(m / PATH_STEP)))];
  const dot = cursor != null ? at(cursor) : null;

  return (
    <svg viewBox={view.box} className="h-full w-full" preserveAspectRatio="xMidYMid meet">
      <polyline
        points={line(reference)}
        fill="none"
        stroke="var(--faint)"
        strokeWidth={9}
        strokeOpacity={0.35}
        strokeLinejoin="round"
        strokeLinecap="round"
        vectorEffect="non-scaling-stroke"
      />
      {review.sections.map((s, i) => {
        const pts = lap.slice(Math.floor(s.start / PATH_STEP), Math.ceil(s.end / PATH_STEP) + 1);
        const on = selected === i;
        return (
          <polyline
            key={i}
            points={line(pts)}
            fill="none"
            stroke={lossColor(s.lost)}
            strokeWidth={on ? 5 : 2.5}
            strokeLinejoin="round"
            strokeLinecap="round"
            vectorEffect="non-scaling-stroke"
            className="cursor-pointer"
            onClick={() => onPick(i)}
          >
            <title>{s.name}</title>
          </polyline>
        );
      })}
      {review.sections.map((s, i) => {
        const label = short(s.name);
        if (!label) return null;
        const p = refAt((s.core[0] + s.core[1]) / 2);
        return (
          <text
            key={`l${i}`}
            x={p[0]}
            y={-p[1]}
            fontSize={view.size / 32}
            textAnchor="middle"
            dominantBaseline="central"
            fill={selected === i ? "var(--foreground)" : "var(--muted-foreground)"}
            stroke="var(--background)"
            strokeWidth={view.size / 180}
            paintOrder="stroke"
            className="cursor-pointer select-none font-semibold"
            onClick={() => onPick(i)}
          >
            {label}
          </text>
        );
      })}
      {dot && (
        <circle cx={dot[0]} cy={-dot[1]} r={view.size / 110} fill="var(--primary)" stroke="var(--background)" strokeWidth={view.size / 400} />
      )}
    </svg>
  );
}
