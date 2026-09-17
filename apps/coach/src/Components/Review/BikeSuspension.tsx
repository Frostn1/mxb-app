import { useMemo } from "react";
import { useT } from "@/i18n";
import type { Channels } from "@/api/coach";
import { Bike } from "./BikeFeel";
import { Label } from "../Page";

/** Where the travel is drawn on the bike from `BikeFeel`, in that drawing's own 360x200 space:
 *  down the fork leg, and along the shock under the seat. Taken from the lines the drawing
 *  already uses for those parts, so the gauge sits on the part rather than beside it. */
const FORK = { x1: 228, y1: 44, x2: 272, y2: 134 };
const SHOCK = { x1: 158, y1: 94, x2: 150, y2: 76 };

/** A travel gauge laid along one part: the whole stroke in grey, the share used filled in, and
 *  a mark at the deepest it went. A bar beside the bike says the same thing, but a rider reads
 *  "the fork is using all of it" off the fork. */
function Travel({
  at,
  used,
  deepest,
  label,
}: {
  at: { x1: number; y1: number; x2: number; y2: number };
  used: number;
  deepest: number;
  label: string;
}) {
  const lerp = (t: number) => ({ x: at.x1 + (at.x2 - at.x1) * t, y: at.y1 + (at.y2 - at.y1) * t });
  const end = lerp(Math.max(0, Math.min(1, used)));
  const mark = lerp(Math.max(0, Math.min(1, deepest)));
  // Across the part, so the mark reads as a line on a scale rather than more fill.
  const dx = at.x2 - at.x1;
  const dy = at.y2 - at.y1;
  const len = Math.hypot(dx, dy) || 1;
  const nx = (-dy / len) * 6;
  const ny = (dx / len) * 6;
  return (
    <g>
      <title>{label}</title>
      <line x1={at.x1} y1={at.y1} x2={at.x2} y2={at.y2} stroke="var(--border)" strokeWidth={9} strokeLinecap="round" />
      <line x1={at.x1} y1={at.y1} x2={end.x} y2={end.y} stroke="var(--primary)" strokeWidth={9} strokeLinecap="round" />
      {deepest > 0.01 && (
        <line
          x1={mark.x - nx}
          y1={mark.y - ny}
          x2={mark.x + nx}
          y2={mark.y + ny}
          stroke="var(--destructive)"
          strokeWidth={2.5}
          strokeLinecap="round"
        />
      )}
    </g>
  );
}

/** The lap's suspension, on the bike rather than as two rows in a list.
 *
 *  `fork` and `shock` are the share of each end's travel in use, per metre of the lap. What a
 *  rider wants off a lap is how much of the stroke they used and whether they ran out of it, so
 *  the fill is the average and the red mark is the deepest point of the lap. */
export default function BikeSuspension({ channels }: { channels: Channels }) {
  const t = useT();
  const stats = useMemo(() => {
    const of = (v: number[]) => {
      const real = v.filter((x) => Number.isFinite(x));
      if (real.length === 0) return null;
      const mean = real.reduce((a, b) => a + b, 0) / real.length / 100;
      const deep = Math.max(...real) / 100;
      return { mean, deep };
    };
    return { fork: of(channels.fork.lap), shock: of(channels.shock.lap) };
  }, [channels]);

  // All zero is how the recorder says it couldn't work the travel out, rather than a lap that
  // never moved the suspension.
  const known = stats.fork && stats.shock && (stats.fork.deep > 0 || stats.shock.deep > 0);
  if (!known) return null;
  const pct = (v: number) => `${Math.round(v * 100)}%`;
  return (
    <div>
      <Label>{t("susp.title")}</Label>
      <div className="border border-border bg-card px-4 py-3">
        <p className="text-[12.5px] text-muted-foreground">{t("susp.body")}</p>
        <div className="relative mt-2">
          <Bike lit={[]} />
          <svg viewBox="0 0 360 200" aria-hidden="true" className="absolute inset-0 h-auto w-full" fill="none">
            <Travel
              at={FORK}
              used={stats.fork!.mean}
              deepest={stats.fork!.deep}
              label={t("susp.fork", { used: pct(stats.fork!.mean), deep: pct(stats.fork!.deep) })}
            />
            <Travel
              at={SHOCK}
              used={stats.shock!.mean}
              deepest={stats.shock!.deep}
              label={t("susp.shock", { used: pct(stats.shock!.mean), deep: pct(stats.shock!.deep) })}
            />
          </svg>
        </div>
        <dl className="mt-2 grid grid-cols-2 gap-x-6 gap-y-1 text-[12.5px]">
          <dt className="text-muted-foreground">{t("susp.front")}</dt>
          <dd className="font-mono">{t("susp.reading", { used: pct(stats.fork!.mean), deep: pct(stats.fork!.deep) })}</dd>
          <dt className="text-muted-foreground">{t("susp.rear")}</dt>
          <dd className="font-mono">{t("susp.reading", { used: pct(stats.shock!.mean), deep: pct(stats.shock!.deep) })}</dd>
        </dl>
      </div>
    </div>
  );
}
