import { useRef, type MouseEvent, type ReactNode } from "react";
import { Button } from "@frost/shared/Components/ui/button";
import { useT } from "@/i18n";
import type { Review } from "@/api/coach";
import { gap } from "@/lib/format";

const W = 1000;
/** Metres of run-up and run-out shown either side of a section. */
const MARGIN = 25;
/** Left column holding each chart's scale, so every plot shares one x axis. */
const GUTTER = 64;

/** Runs where `top` stays above (or below) `bottom`, as filled shapes between the two. */
function bands(xs: number[], top: number[], bottom: number[], y: (v: number) => number) {
  const out: { up: boolean; points: string }[] = [];
  const up = (i: number) => top[i] >= bottom[i];
  let s = 0;
  for (let i = 1; i <= xs.length; i++) {
    if (i < xs.length && up(i) === up(s)) continue;
    const e = Math.min(i, xs.length - 1);
    const fwd: string[] = [];
    const back: string[] = [];
    for (let k = s; k <= e; k++) {
      fwd.push(`${xs[k]},${y(top[k])}`);
      back.unshift(`${xs[k]},${y(bottom[k])}`);
    }
    out.push({ up: up(s), points: [...fwd, ...back].join(" ") });
    s = i;
  }
  return out;
}

const poly = (xs: number[], vs: number[], y: (v: number) => number) => xs.map((x, k) => `${x},${y(vs[k])}`).join(" ");

function scale(vs: number[], h: number, lo?: number, hi?: number) {
  const min = lo ?? Math.min(...vs);
  const max = hi ?? Math.max(...vs);
  const span = max - min || 1;
  return { min, max, y: (v: number) => h - 4 - ((v - min) / span) * (h - 8) };
}

function Row({
  label,
  hint,
  value,
  gutter,
  height,
  children,
}: {
  label: string;
  hint?: string;
  value?: ReactNode;
  gutter?: ReactNode;
  height: number;
  children: ReactNode;
}) {
  return (
    <div className="border-t border-border first:border-t-0">
      <div className="flex items-baseline justify-between gap-3 px-3 pb-1 pt-2 text-[11px]">
        <span>
          <span className="font-semibold">{label}</span>
          {hint && <span className="ml-2 text-faint">{hint}</span>}
        </span>
        {value && <span className="font-mono tabular-nums text-muted-foreground">{value}</span>}
      </div>
      <div className="flex">
        <div className="relative shrink-0 font-mono text-[10px] text-faint" style={{ width: GUTTER, height }}>
          {gutter}
        </div>
        <svg viewBox={`0 0 ${W} ${height}`} preserveAspectRatio="none" style={{ height }} className="block min-w-0 flex-1">
          {children}
        </svg>
      </div>
    </div>
  );
}

/** Scale labels at the top and bottom of a chart's gutter. */
function Ends({ top, bottom }: { top: string; bottom: string }) {
  return (
    <>
      <span className="absolute right-2 top-0">{top}</span>
      <span className="absolute bottom-0 right-2">{bottom}</span>
    </>
  );
}

const stroke = { fill: "none", vectorEffect: "non-scaling-stroke" as const, strokeLinejoin: "round" as const };

/**
 * Speed, brake and gas, and time, for you against the fast lap, zoomed to the picked section.
 * Numbered lines mark where each piece of advice applies.
 */
export default function Charts({
  review,
  selected,
  whole,
  onWhole,
  cursor,
  onCursor,
}: {
  review: Review;
  selected: number | null;
  whole: boolean;
  onWhole: (whole: boolean) => void;
  cursor: number | null;
  onCursor: (m: number | null) => void;
}) {
  const t = useT();
  const plot = useRef<HTMLDivElement>(null);
  const ch = review.channels;
  const step = ch.step;
  const n = ch.delta.length;
  const length = (n - 1) * step;
  const sel = selected != null ? review.sections[selected] : null;
  const zoomed = sel != null && !whole;
  const a = zoomed ? Math.max(0, sel.start - MARGIN) : 0;
  const b = zoomed ? Math.min(length, sel.end + MARGIN) : length;
  const ia = Math.floor(a / step);
  const ib = Math.min(n - 1, Math.ceil(b / step));
  const idx = Array.from({ length: ib - ia + 1 }, (_, k) => ia + k);
  const span = Math.max(b - a, 1);
  const xs = idx.map((i) => ((i * step - a) / span) * W);
  const pct = (m: number) => ((m - a) / span) * 100;
  const pick = (v: number[]) => idx.map((i) => v[i]);
  const inView = cursor != null && cursor >= a && cursor <= b;
  const at = inView ? Math.round(cursor / step) : null;

  const you = pick(ch.speed.lap);
  const fast = pick(ch.speed.reference);
  const sp = scale([...you, ...fast], 130);
  const lost = idx.map((i) => ch.delta[i] - ch.delta[ia]);
  const lim = Math.max(0.05, ...lost.map(Math.abs));
  const tm = scale(lost, 80, -lim, lim);
  const end = lost[lost.length - 1] ?? 0;
  const leanYou = pick(ch.lean.lap).map(Math.abs);
  const leanFast = pick(ch.lean.reference).map(Math.abs);
  const ln = scale([...leanYou, ...leanFast], 60, 0);
  const hasTravel = ch.fork.lap.some((v) => v > 0);
  const tv = scale([0], 60, 0, 110);
  const tick = span < 150 ? 25 : span < 400 ? 50 : span < 1200 ? 100 : 200;
  const ticks: number[] = [];
  for (let m = Math.ceil(a / tick) * tick; m <= b; m += tick) ticks.push(m);

  // Notes at the same spot stack instead of hiding each other.
  const markers: { n: number; at: number; row: number }[] = [];
  if (zoomed) {
    sel.findings.forEach((f, k) => {
      if (f.at < a || f.at > b) return;
      const row = markers.filter((m) => Math.abs(pct(m.at) - pct(f.at)) < 2).length;
      markers.push({ n: k + 1, at: f.at, row });
    });
  }

  const move = (e: MouseEvent<HTMLDivElement>) => {
    const r = plot.current?.getBoundingClientRect();
    if (!r || e.clientX < r.left) return onCursor(null);
    onCursor(a + Math.max(0, Math.min(1, (e.clientX - r.left) / r.width)) * span);
  };

  const lane = (throttle: number[], brake: number[], y0: number, h: number) => {
    const base = y0 + h;
    const area = (vs: number[]) => `0,${base} ${xs.map((x, k) => `${x},${base - vs[k] * h}`).join(" ")} ${W},${base}`;
    return (
      <>
        <rect x={0} y={y0} width={W} height={h} fill="var(--foreground)" fillOpacity={0.04} />
        <polygon points={area(throttle)} fill="var(--success)" fillOpacity={0.65} />
        <polygon points={area(brake)} fill="var(--destructive)" fillOpacity={0.85} />
      </>
    );
  };

  return (
    <div>
      <div className="flex flex-wrap items-center justify-between gap-3 border-b border-border px-3 py-2">
        <div className="flex items-center gap-4 text-[12px]">
          <span className="font-semibold">{zoomed ? `${sel.name} ${t("review.upClose")}` : t("review.wholeLap")}</span>
          <span className="flex items-center gap-1.5 text-muted-foreground">
            <span className="inline-block h-[3px] w-4 rounded bg-primary" />
            {t("review.you")}
          </span>
          <span className="flex items-center gap-1.5 text-muted-foreground">
            <span className="inline-block h-[3px] w-4 rounded bg-faint" />
            {t("review.fastLap")}
          </span>
        </div>
        {sel && (
          <Button size="sm" variant="outline" onClick={() => onWhole(!whole)}>
            {whole ? t("review.showSection") : t("review.showWhole")}
          </Button>
        )}
      </div>

      <div className="relative select-none" onMouseMove={move} onMouseLeave={() => onCursor(null)}>
        <Row
          label={t("review.speed")}
          hint={t("review.speedHint")}
          height={130}
          value={at != null ? `${ch.speed.lap[at].toFixed(0)} / ${ch.speed.reference[at].toFixed(0)} km/h` : undefined}
          gutter={<Ends top={`${sp.max.toFixed(0)} km/h`} bottom={`${sp.min.toFixed(0)}`} />}
        >
          {bands(xs, you, fast, sp.y).map((band, k) => (
            <polygon key={k} points={band.points} fill={band.up ? "var(--success)" : "var(--destructive)"} fillOpacity={0.25} />
          ))}
          <polyline points={poly(xs, fast, sp.y)} stroke="var(--faint)" strokeWidth={1.5} {...stroke} />
          <polyline points={poly(xs, you, sp.y)} stroke="var(--primary)" strokeWidth={2} {...stroke} />
        </Row>

        <Row
          label={t("review.inputs")}
          hint={t("review.inputsHint")}
          height={60}
          gutter={
            <>
              <span className="absolute right-2 top-[9px] font-sans font-semibold text-muted-foreground">{t("review.you")}</span>
              <span className="absolute right-2 top-[39px] font-sans font-semibold text-muted-foreground">{t("review.fastLap")}</span>
            </>
          }
        >
          {lane(pick(ch.throttle.lap), pick(ch.brake.lap), 4, 24)}
          {lane(pick(ch.throttle.reference), pick(ch.brake.reference), 34, 24)}
        </Row>

        <Row
          label={zoomed ? t("review.timeHere") : t("review.time")}
          hint={t("review.timeHint")}
          height={80}
          value={
            <span style={{ color: end > 0.005 ? "var(--destructive)" : end < -0.005 ? "var(--success)" : undefined }}>
              {gap(at != null ? ch.delta[at] - ch.delta[ia] : end)} s
            </span>
          }
          gutter={<Ends top={`+${lim.toFixed(2)} s`} bottom={`−${lim.toFixed(2)}`} />}
        >
          <line x1={0} x2={W} y1={tm.y(0)} y2={tm.y(0)} stroke="var(--border)" vectorEffect="non-scaling-stroke" />
          {bands(xs, lost, lost.map(() => 0), tm.y).map((band, k) => (
            <polygon key={k} points={band.points} fill={band.up ? "var(--destructive)" : "var(--success)"} fillOpacity={0.3} />
          ))}
          <polyline points={poly(xs, lost, tm.y)} stroke="var(--foreground)" strokeOpacity={0.8} strokeWidth={1.5} {...stroke} />
        </Row>

        <Row
          label={t("review.lean")}
          height={60}
          value={at != null ? `${Math.abs(ch.lean.lap[at]).toFixed(0)}° / ${Math.abs(ch.lean.reference[at]).toFixed(0)}°` : undefined}
          gutter={<Ends top={`${ln.max.toFixed(0)}°`} bottom="0°" />}
        >
          <polyline points={poly(xs, leanFast, ln.y)} stroke="var(--faint)" strokeWidth={1.5} {...stroke} />
          <polyline points={poly(xs, leanYou, ln.y)} stroke="var(--primary)" strokeWidth={2} {...stroke} />
        </Row>

        {hasTravel && (
          <Row
            label={t("review.travel")}
            hint={t("review.travelHint")}
            height={60}
            value={at != null ? `${ch.fork.lap[at].toFixed(0)}% / ${ch.shock.lap[at].toFixed(0)}%` : undefined}
            gutter={<Ends top="100%" bottom="0%" />}
          >
            <line x1={0} x2={W} y1={tv.y(95)} y2={tv.y(95)} stroke="var(--destructive)" strokeOpacity={0.7} strokeDasharray="6 5" vectorEffect="non-scaling-stroke" />
            <polyline points={poly(xs, pick(ch.shock.lap), tv.y)} stroke="var(--warning)" strokeWidth={1.5} {...stroke} />
            <polyline points={poly(xs, pick(ch.fork.lap), tv.y)} stroke="var(--primary)" strokeWidth={1.5} {...stroke} />
          </Row>
        )}

        <div ref={plot} className="pointer-events-none absolute bottom-0 right-0 top-0" style={{ left: GUTTER }}>
          {markers.map((m) => (
            <div key={m.n} className="absolute bottom-0 top-0 border-l border-dashed border-foreground/60" style={{ left: `${pct(m.at)}%` }}>
              <span
                className="absolute -left-2 flex size-4 items-center justify-center rounded-full bg-foreground text-[10px] font-bold text-background"
                style={{ top: 26 + m.row * 20 }}
              >
                {m.n}
              </span>
            </div>
          ))}
          {inView && <div className="absolute bottom-0 top-0 border-l border-foreground/70" style={{ left: `${pct(cursor)}%` }} />}
        </div>
      </div>

      <div className="relative h-6 border-t border-border" style={{ marginLeft: GUTTER }}>
        {ticks.map((m) => (
          <span key={m} className="absolute top-1 -translate-x-1/2 font-mono text-[10px] text-faint" style={{ left: `${pct(m)}%` }}>
            {m} m
          </span>
        ))}
      </div>
    </div>
  );
}
