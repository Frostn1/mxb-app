import type { MouseEvent } from "react";
import { useT, type TKey } from "@/i18n";
import type { Channel, Review } from "@/api/coach";

const W = 1000;
const H = 100;

type Row = { key: TKey; channel?: Channel; delta?: number[]; fixed?: [number, number]; digits: number };

/**
 * The lap's inputs against the reference's, stacked on one distance axis: the running gap,
 * speed, throttle, brake and lean. Hovering moves a cursor shared with the map.
 */
export default function Traces({
  review,
  selected,
  cursor,
  onCursor,
}: {
  review: Review;
  selected: number | null;
  cursor: number | null;
  onCursor: (m: number | null) => void;
}) {
  const t = useT();
  const ch = review.channels;
  const n = ch.delta.length;
  const length = (n - 1) * ch.step;
  const xOf = (m: number) => (m / Math.max(length, 1)) * W;
  const rows: Row[] = [
    { key: "review.gap", delta: ch.delta, digits: 2 },
    { key: "review.speed", channel: ch.speed, digits: 0 },
    { key: "review.throttle", channel: ch.throttle, fixed: [0, 1], digits: 2 },
    { key: "review.brake", channel: ch.brake, fixed: [0, 1], digits: 2 },
    { key: "review.lean", channel: ch.lean, digits: 0 },
  ];
  const sel = selected != null ? review.sections[selected] : null;
  const idx = cursor != null ? Math.min(n - 1, Math.max(0, Math.round(cursor / ch.step))) : null;

  const move = (e: MouseEvent<HTMLDivElement>) => {
    const r = e.currentTarget.getBoundingClientRect();
    onCursor(Math.max(0, Math.min(1, (e.clientX - r.left) / r.width)) * length);
  };

  return (
    <div className="select-none" onMouseMove={move} onMouseLeave={() => onCursor(null)}>
      {rows.map((row) => {
        const series = row.delta ? [row.delta] : [row.channel!.reference, row.channel!.lap];
        const all = series.flat();
        let [lo, hi] = row.fixed ?? [Math.min(...all), Math.max(...all)];
        if (row.delta) {
          const m = Math.max(Math.abs(lo), Math.abs(hi), 0.05);
          [lo, hi] = [-m, m];
        }
        const span = hi - lo || 1;
        const yOf = (v: number) => H - ((v - lo) / span) * (H - 8) - 4;
        const line = (vs: number[]) => vs.map((v, i) => `${(i / Math.max(n - 1, 1)) * W},${yOf(v)}`).join(" ");
        const readout =
          idx == null
            ? ""
            : row.delta
              ? `${row.delta[idx] > 0 ? "+" : ""}${row.delta[idx].toFixed(row.digits)}`
              : `${row.channel!.lap[idx].toFixed(row.digits)} / ${row.channel!.reference[idx].toFixed(row.digits)}`;
        return (
          <div key={row.key} className="relative border-b border-border last:border-b-0">
            <div className="pointer-events-none absolute left-2 top-1 z-10 text-[10.5px] font-semibold uppercase tracking-[0.08em] text-faint">
              {t(row.key)}
            </div>
            <div className="pointer-events-none absolute right-2 top-1 z-10 font-mono text-[11px] tabular-nums text-muted-foreground">
              {readout}
            </div>
            <svg viewBox={`0 0 ${W} ${H}`} preserveAspectRatio="none" className="block h-[72px] w-full">
              {sel && (
                <rect x={xOf(sel.start)} y={0} width={Math.max(1, xOf(sel.end) - xOf(sel.start))} height={H} fill="var(--accent)" />
              )}
              {row.delta && (
                <line x1={0} x2={W} y1={yOf(0)} y2={yOf(0)} stroke="var(--border)" vectorEffect="non-scaling-stroke" />
              )}
              {series.map((vs, k) => (
                <polyline
                  key={k}
                  points={line(vs)}
                  fill="none"
                  stroke={row.delta ? "var(--warning)" : k === 0 ? "var(--faint)" : "var(--primary)"}
                  strokeWidth={k === 0 && !row.delta ? 1 : 1.5}
                  vectorEffect="non-scaling-stroke"
                />
              ))}
              {cursor != null && (
                <line x1={xOf(cursor)} x2={xOf(cursor)} y1={0} y2={H} stroke="var(--foreground)" strokeOpacity={0.5} vectorEffect="non-scaling-stroke" />
              )}
            </svg>
          </div>
        );
      })}
    </div>
  );
}
