import { useEffect, useMemo, useState } from "react";
import { loadTrackOverview, loadTrackTerrain } from "@frost/shared/api/tracks";
import type { TrackOverview, TrackTerrain } from "@frost/shared/types";
import { ChevronLeft, ChevronRight } from "lucide-react";
import { Button } from "@frost/shared/Components/ui/button";
import { Segmented } from "@frost/shared/Components/ui/segmented";
import { cn } from "@frost/shared/lib/utils";
import { useT, type TKey } from "@/i18n";
import {
  coachGround,
  coachLines,
  coachReview,
  coachSurface,
  type Finding,
  type Lines,
  type ReviewOut,
  type SectionReview,
  type Surface,
  type Theme,
} from "@/api/coach";
import { gap, lapTime, lossColor, started } from "@/lib/format";
import Page, { Label } from "../Page";
import TrackMap from "./TrackMap";
import Track3D, { surfaceTerrain, type Ground3D } from "./Track3D";
import SectionStrip from "./SectionStrip";
import Charts from "./Charts";

/** One lap against the reference, or on its own: where the time went, and what to change. */
export default function Review({
  path,
  lap,
  solo: startAlone = false,
  onBack,
}: {
  path: string;
  lap: number;
  /** Start on its own rather than against the fast lap. */
  solo?: boolean;
  onBack: () => void;
}) {
  const t = useT();
  const [alone, setAlone] = useState(startAlone);
  const [data, setData] = useState<ReviewOut | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [selected, setSelected] = useState<number | null>(null);
  const [cursor, setCursor] = useState<number | null>(null);
  const [whole, setWhole] = useState(false);
  const [surface, setSurface] = useState<Surface | null>(null);
  const [view, setView] = useState<"map" | "laps" | "3d">("map");
  const [lines, setLines] = useState<Lines | null>(null);
  const [real, setReal] = useState<{ terrain: TrackTerrain; overview: TrackOverview | null; lift: number } | null>(null);

  // The ground and the other laps are extras: the review stands without them. The track's
  // own terrain wins over the ground built from the laps, when it's there and lines up.
  useEffect(() => {
    setSurface(null);
    setLines(null);
    setReal(null);
    coachSurface(path).then(setSurface).catch(() => {});
    coachLines(path).then(setLines).catch(() => {});
    coachGround(path)
      .then(async (g) => {
        if (!g) return;
        const [terrain, overview] = await Promise.all([
          loadTrackTerrain(g.path, 1024, g.prefix),
          loadTrackOverview(g.path, 1024, g.prefix).catch(() => null),
        ]);
        setReal({ terrain, overview, lift: g.lift });
      })
      .catch(() => {});
  }, [path]);

  const relief = useMemo(
    () =>
      real
        ? { x0: 0, z0: 0, cell: real.terrain.metresPerSample, width: real.terrain.width, height: real.terrain.height, heights: real.terrain.heights }
        : surface,
    [real, surface],
  );
  const ground3d = useMemo<Ground3D | null>(
    () =>
      real
        ? { terrain: real.terrain, overview: real.overview, origin: [0, 0], lift: real.lift }
        : surface
          ? { terrain: surfaceTerrain(surface), overview: null, origin: [surface.x0, surface.z0], lift: 0 }
          : null,
    [real, surface],
  );

  useEffect(() => {
    setData(null);
    setError(null);
    coachReview(path, lap, undefined, undefined, alone)
      .then((r) => {
        setData(r);
        setSelected(r.review.focus[0] ?? null);
      })
      .catch((e) => setError(String(e)));
  }, [path, lap, alone]);

  const count = data?.review.sections.length ?? 0;
  const pick = (i: number) => {
    setSelected(i);
    setWhole(false);
  };
  useEffect(() => {
    const key = (e: KeyboardEvent) => {
      if (!count || (e.key !== "ArrowLeft" && e.key !== "ArrowRight")) return;
      const d = e.key === "ArrowLeft" ? -1 : 1;
      setSelected((s) => ((s ?? (d > 0 ? -1 : 0)) + d + count) % count);
      setWhole(false);
    };
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  }, [count]);

  const back = t("review.back");
  if (error || !data) {
    return (
      <Page title={t("review.title")} onBack={onBack} backLabel={back}>
        <p className="text-[13px] text-muted-foreground">{error ?? t("common.loading")}</p>
      </Page>
    );
  }

  const { review, reference } = data;
  const solo = review.solo;
  const total = (data.lap.timeMs - reference.timeMs) / 1000;
  const sel = selected != null ? review.sections[selected] : null;
  // Where the time went first, then anything flagged that cost nothing, like a hard landing.
  const worth = [
    ...review.focus,
    ...review.sections.map((s, i) => (s.findings.length > 0 && !review.focus.includes(i) ? i : -1)).filter((i) => i >= 0),
  ];
  const against = [lapTime(reference.timeMs), reference.bikeName, started(reference.started)].filter(Boolean).join(" · ");
  // Every lap's line, fastest green to slowest red; this lap in blue on top.
  const others = (() => {
    if (view !== "laps" || !lines) return undefined;
    const times = lines.laps.map((l) => l.time);
    const [lo, hi] = [Math.min(...times), Math.max(...times)];
    return lines.laps
      .filter((l) => l.lap !== lap)
      .map((l) => ({ path: l.path, colour: `hsl(${Math.round(120 * (1 - (l.time - lo) / Math.max(hi - lo, 0.01)))} 65% 55%)` }));
  })();
  const views = [
    { value: "map" as const, label: t("review.viewMap") },
    ...(lines && lines.laps.length > 1 ? [{ value: "laps" as const, label: t("review.viewLaps") }] : []),
    ...(ground3d ? [{ value: "3d" as const, label: t("review.view3d") }] : []),
  ];
  const bySection = (name: string) => {
    const i = review.sections.findIndex((s) => s.name === name);
    if (i >= 0) pick(i);
  };

  return (
    <Page
      wide
      title={`${t("session.lap")} ${lap + 1} · ${lapTime(data.lap.timeMs)}`}
      sub={
        solo ? (
          t("review.aloneSub")
        ) : (
          <>
            <span style={{ color: lossColor(total) }} className="font-mono">
              {gap(total)} s
            </span>{" "}
            {t("review.against")} {against}
          </>
        )
      }
      actions={
        (!solo || alone) && (
          <Segmented
            size="sm"
            value={alone ? "alone" : "compare"}
            onChange={(v) => setAlone(v === "alone")}
            options={[
              { value: "compare", label: t("review.compare") },
              { value: "alone", label: t("review.alone") },
            ]}
          />
        )
      }
      onBack={onBack}
      backLabel={back}
    >
      <div className="grid gap-6 xl:grid-cols-[minmax(0,1fr)_400px]">
        <div className="min-w-0 space-y-3">
          <div className="relative h-[440px] border border-border bg-card">
            {views.length > 1 && (
              <Segmented size="sm" className="absolute right-3 top-3 z-10" value={view} onChange={setView} options={views} />
            )}
            {view === "3d" && ground3d ? (
              <Track3D review={review} ground={ground3d} selected={selected} className="h-full w-full" />
            ) : (
              <div className="h-full p-3">
                <TrackMap review={review} surface={relief} others={others} selected={selected} cursor={cursor} onPick={pick} solo={solo} />
              </div>
            )}
            {view === "laps" && (
              <p className="pointer-events-none absolute bottom-2 left-3 text-[11px] text-muted-foreground">{t("review.lapsLegend")}</p>
            )}
          </div>
          <SectionStrip review={review} selected={selected} onPick={pick} />
          <p className="text-[11.5px] text-faint">{t("review.strip")}</p>
          <div className="mt-3 border border-border bg-card">
            <Charts review={review} selected={selected} whole={whole} onWhole={setWhole} cursor={cursor} onCursor={setCursor} />
          </div>
        </div>

        <div className="space-y-6">
          <Overall themes={review.overall} solo={solo} onPick={bySection} />

          {sel && selected != null && (
            <SectionPanel
              key={sel.name}
              s={sel}
              solo={solo}
              onPrev={() => pick((selected - 1 + count) % count)}
              onNext={() => pick((selected + 1) % count)}
            />
          )}

          {review.setup.length > 0 && <Setup findings={review.setup} />}

          <div>
            <Label>{t("review.focus")}</Label>
            {worth.length === 0 ? (
              <p className="border border-border px-4 py-3 text-[12.5px] text-muted-foreground">{t("review.nothing")}</p>
            ) : (
              <div className="space-y-1">
                {worth.map((i, k) => {
                  const s = review.sections[i];
                  return (
                    <button
                      key={i}
                      onClick={() => pick(i)}
                      className={cn(
                        "flex w-full items-center gap-3 border bg-card px-3 py-2 text-left",
                        selected === i ? "border-primary/60" : "border-border hover:border-foreground/30",
                      )}
                    >
                      <span className="font-mono text-[11px] text-faint">{k + 1}</span>
                      <span className="w-20 shrink-0 text-[12.5px] font-semibold">{s.name}</span>
                      <span className="min-w-0 flex-1 truncate text-[12px] text-muted-foreground">{s.findings[0]?.title}</span>
                      {!solo && (
                        <span className="font-mono text-[12px] tabular-nums" style={{ color: lossColor(s.lost) }}>
                          {gap(s.lost)}
                        </span>
                      )}
                    </button>
                  );
                })}
              </div>
            )}
          </div>

          {lines && lines.notes.length > 0 && (
            <div>
              <Label>{t("review.linesTitle")}</Label>
              <div className="space-y-1">
                {lines.notes.map((n, k) => (
                  <button
                    key={k}
                    onClick={() => bySection(n.name)}
                    className={cn(
                      "w-full border bg-card px-4 py-3 text-left",
                      sel?.name === n.name ? "border-primary/60" : "border-border hover:border-foreground/30",
                    )}
                  >
                    <div className="text-[13px] font-semibold">{n.title}</div>
                    <div className="mt-0.5 text-[12.5px] leading-snug text-muted-foreground">{n.detail}</div>
                  </button>
                ))}
              </div>
            </div>
          )}
        </div>
      </div>
    </Page>
  );
}

/** The lap in a few lines: each kind of mistake, where it happened, and the tip for it. */
function Overall({ themes, solo, onPick }: { themes: Theme[]; solo: boolean; onPick: (section: string) => void }) {
  const t = useT();
  if (themes.length === 0) return null;
  return (
    <div>
      <Label>{t("review.overall")}</Label>
      <div className="divide-y divide-border border border-border bg-card">
        {themes.map((th) => (
          <button key={th.name} onClick={() => onPick(th.sections[0])} className="flex w-full items-start gap-3 px-4 py-2.5 text-left hover:bg-accent">
            <div className="min-w-0 flex-1">
              <div className="flex items-baseline gap-2">
                <span className="text-[13px] font-semibold">{th.name}</span>
                <span className="truncate text-[11.5px] text-faint">{th.sections.join(", ")}</span>
              </div>
              <div className="mt-0.5 text-[12px] text-muted-foreground">{th.tip}</div>
            </div>
            {!solo && (
              <span className="font-mono text-[12px] tabular-nums" style={{ color: lossColor(th.lost) }}>
                {gap(th.lost)}
              </span>
            )}
          </button>
        ))}
      </div>
    </div>
  );
}

const SETUP_GROUPS: { key: TKey; of: (skill: string) => boolean }[] = [
  { key: "review.group.suspension", of: (s) => s === "setup_bottoming" || s === "setup_stiff" },
  { key: "review.group.gearing", of: (s) => s.startsWith("setup_gearing") },
  { key: "review.group.shifting", of: (s) => s.startsWith("setup_shift") },
  { key: "review.group.chassis", of: (s) => s === "setup_swingarm" },
];

/** Bike setup advice for the whole lap, by what it's about. */
function Setup({ findings }: { findings: Finding[] }) {
  const t = useT();
  return (
    <div>
      <Label>{t("review.setup")}</Label>
      <div className="space-y-4 border border-border bg-card px-4 py-3">
        {SETUP_GROUPS.map((g) => {
          const mine = findings.filter((f) => g.of(f.skill));
          if (mine.length === 0) return null;
          return (
            <div key={g.key}>
              <div className="mb-1.5 text-[10.5px] font-semibold uppercase tracking-[0.09em] text-faint">{t(g.key)}</div>
              <Notes findings={mine} />
            </div>
          );
        })}
      </div>
    </div>
  );
}

/** The picked section: what it cost, the tip that matters most, and the rest underneath. */
function SectionPanel({ s, solo, onPrev, onNext }: { s: SectionReview; solo: boolean; onPrev: () => void; onNext: () => void }) {
  const t = useT();
  const [more, setMore] = useState(false);
  const [head, ...rest] = s.findings;
  return (
    <div className="border border-primary/40 bg-card">
      <div className="flex items-start justify-between gap-3 border-b border-border px-4 py-3">
        <div>
          <div className="text-[10.5px] font-semibold uppercase tracking-[0.09em] text-faint">
            {t(`review.kind.${s.kind}` as TKey)}
          </div>
          <div className="mt-0.5 font-cond text-[22px] font-semibold leading-tight">{s.name}</div>
          <div className="mt-1 text-[12px] text-muted-foreground">
            {t("review.you")} {s.lapTime.toFixed(2)} s
            {!solo && ` · ${t("review.fastLap")} ${s.refTime.toFixed(2)} s`}
          </div>
        </div>
        <div className="text-right">
          {!solo && (
            <div className="font-mono text-[20px] tabular-nums" style={{ color: lossColor(s.lost) }}>
              {gap(s.lost)}
            </div>
          )}
          <div className="mt-2 flex gap-1">
            <Button size="sm" variant="outline" onClick={onPrev} aria-label={t("review.prev")}>
              <ChevronLeft className="size-3.5" />
            </Button>
            <Button size="sm" variant="outline" onClick={onNext} aria-label={t("review.next")}>
              <ChevronRight className="size-3.5" />
            </Button>
          </div>
        </div>
      </div>
      <div className="px-4 py-3">
        {!head ? (
          <p className="text-[12.5px] text-muted-foreground">{t("review.nothingHere")}</p>
        ) : (
          <>
            <div className="flex gap-2.5">
              <Num n={1} />
              <div>
                <div className="text-[14px] font-semibold">{head.title}</div>
                <div className="mt-1 text-[12.5px] leading-snug text-muted-foreground">{head.detail}</div>
              </div>
            </div>
            {rest.length > 0 && (
              <>
                <button onClick={() => setMore(!more)} className="mt-3 text-[12px] font-medium text-primary hover:underline">
                  {more ? t("review.lessDetail") : `${t("review.moreDetail")} (${rest.length})`}
                </button>
                {more && (
                  <div className="mt-3">
                    <Notes findings={rest} start={2} />
                  </div>
                )}
              </>
            )}
          </>
        )}
      </div>
    </div>
  );
}

function Num({ n }: { n: number }) {
  return (
    <span className="mt-0.5 flex size-4 shrink-0 items-center justify-center rounded-full bg-foreground text-[10px] font-bold text-background">
      {n}
    </span>
  );
}

/** Tips as a list; numbered from `start` when they match chart markers. */
function Notes({ findings, start }: { findings: Finding[]; start?: number }) {
  return (
    <ul className="space-y-3">
      {findings.map((f, k) => (
        <li key={k} className="flex gap-2.5">
          {start != null && <Num n={start + k} />}
          <div>
            <div className="text-[13px] font-semibold">{f.title}</div>
            <div className="mt-0.5 text-[12.5px] leading-snug text-muted-foreground">{f.detail}</div>
          </div>
        </li>
      ))}
    </ul>
  );
}
