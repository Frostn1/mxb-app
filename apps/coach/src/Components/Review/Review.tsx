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
  type Ground,
  type Lines,
  type ReviewOut,
  type Rival,
  type SectionReview,
  type Surface,
  type Theme,
} from "@/api/coach";
import { gap, lapTime, lossColor, started } from "@/lib/format";
import { ALONE, refArgs, rememberRef, rememberedRef, type Reference } from "@/lib/reference";
import Page, { Label } from "../Page";
import RefPicker, { ReferenceLine } from "./RefPicker";
import TrackMap from "./TrackMap";
import Track3D from "./Track3D";
import SectionStrip from "./SectionStrip";
import Charts from "./Charts";
import SetupFixes, { Notes, Num } from "./SetupFixes";
import LiveCues from "./LiveCues";

/** One lap against the reference, or on its own: where the time went, and what to change. */
export default function Review({
  path,
  lap,
  trackId,
  solo: startAlone = false,
  onBack,
}: {
  path: string;
  lap: number;
  /** The track this lap is on. The reference is remembered per track. */
  trackId: string;
  /** Start on its own rather than against the fast lap. */
  solo?: boolean;
  onBack: () => void;
}) {
  const t = useT();
  // What the lap is held against; see `lib/reference.ts`. Whatever the rider picked here last
  // time on this track, so they don't pick it again every lap.
  const [compare, setCompare] = useState<Reference>(() => (startAlone ? ALONE : rememberedRef(trackId)));
  const pickRef = (v: Reference) => {
    setCompare(v);
    rememberRef(trackId, v);
  };
  const [data, setData] = useState<ReviewOut | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [selected, setSelected] = useState<number | null>(null);
  const [cursor, setCursor] = useState<number | null>(null);
  const [whole, setWhole] = useState(false);
  const [surface, setSurface] = useState<Surface | null>(null);
  // Two separate choices: which lines are drawn, and whether it's drawn flat or in 3D.
  const [laps, setLaps] = useState<"one" | "all">("one");
  const [dim, setDim] = useState<"2d" | "3d">("2d");
  const [lines, setLines] = useState<Lines | null>(null);
  const [real, setReal] = useState<{ terrain: TrackTerrain; overview: TrackOverview | null; lift: number } | null>(null);
  const [ground, setGround] = useState<Ground | null>(null);
  const [why, setWhy] = useState<string | null>(null);

  // The ground and the other laps are extras: the review stands without them. The track's
  // own terrain wins over the ground built from the laps, when it's there and lines up.
  useEffect(() => {
    setSurface(null);
    setLines(null);
    setReal(null);
    setGround(null);
    setWhy(null);
    coachSurface(path).then(setSurface).catch(() => {});
    coachLines(path).then(setLines).catch(() => {});
    coachGround(path)
      .then(async (answer) => {
        setGround(answer.ground);
        setWhy(answer.why);
        const g = answer.ground;
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

  useEffect(() => {
    setData(null);
    setError(null);
    coachReview(path, lap, refArgs(compare))
      .then((r) => {
        setData(r);
        setSelected(r.review.focus[0] ?? null);
      })
      .catch((e) => setError(String(e)));
  }, [path, lap, compare]);

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
      // The picker is here too: a reference that can't be loaded — a recording deleted, an
      // import removed — would otherwise leave the rider on an error with no way to change it.
      <Page
        title={t("review.title")}
        onBack={onBack}
        backLabel={back}
        actions={<RefPicker trackId={trackId} path={path} lap={lap} bikeId="" value={compare} onChange={pickRef} />}
      >
        <p className="text-[13px] text-muted-foreground">{error ?? t("common.loading")}</p>
      </Page>
    );
  }

  const { review, reference } = data;
  const solo = review.solo;
  // The ideal lap has real section times and no line: nothing to draw the traces or the ghost
  // against, so those are drawn the way they are for a lap reviewed on its own.
  const noTrace = solo || !review.traced;
  const total = (data.lap.timeMs - reference.timeMs) / 1000;
  const sel = selected != null ? review.sections[selected] : null;
  // Where the time went first, then anything flagged that cost nothing, like a hard landing.
  const worth = [
    ...review.focus,
    ...review.sections.map((s, i) => (s.findings.length > 0 && !review.focus.includes(i) ? i : -1)).filter((i) => i >= 0),
  ];
  const where = [data.trackName || data.trackId, started(data.lap.started), data.lap.bikeName].filter(Boolean).join(" · ");
  // Every lap's line, fastest green to slowest red; this lap in blue on top.
  const others = (() => {
    if (laps !== "all" || !lines) return undefined;
    const times = lines.laps.map((l) => l.time);
    const [lo, hi] = [Math.min(...times), Math.max(...times)];
    return lines.laps
      .filter((l) => l.lap !== lap)
      .map((l) => ({ path: l.path, colour: `hsl(${Math.round(120 * (1 - (l.time - lo) / Math.max(hi - lo, 0.01)))} 65% 55%)` }));
  })();
  const canAll = lines != null && lines.laps.length > 1;
  const can3d = ground != null || surface != null;
  const bySection = (name: string) => {
    const i = review.sections.findIndex((s) => s.name === name);
    if (i >= 0) pick(i);
  };

  return (
    <Page
      wide
      title={`${t("session.lap")} ${lap + 1} · ${lapTime(data.lap.timeMs)}`}
      sub={
        <>
          <div className="text-foreground/80">{where}</div>
          <div className="mt-0.5">
            {solo ? (
              t("review.aloneSub")
            ) : (
              <>
                <span style={{ color: lossColor(total) }} className="font-mono">
                  {gap(total)} s
                </span>{" "}
                {t("review.against")}{" "}
                <ReferenceLine reference={reference} lapPath={path} lapBikeId={data.lap.bikeId} idealFrom={data.idealFrom} />
              </>
            )}
          </div>
          {noTrace && !solo && <div className="mt-0.5 text-faint">{t("review.idealSub")}</div>}
        </>
      }
      actions={
        <RefPicker trackId={trackId} path={path} lap={lap} bikeId={data.lap.bikeId} value={compare} onChange={pickRef} />
      }
      onBack={onBack}
      backLabel={back}
    >
      <div className="grid gap-6 xl:grid-cols-[minmax(0,1fr)_400px]">
        <div className="min-w-0 space-y-3">
          <div className="relative h-[440px] border border-border bg-card">
            <div className="absolute right-3 top-3 z-10 flex gap-2">
              {canAll && (
                <Segmented
                  size="sm"
                  value={laps}
                  onChange={setLaps}
                  options={[
                    { value: "one", label: t("review.thisLap") },
                    { value: "all", label: t("review.viewLaps") },
                  ]}
                />
              )}
              {can3d && (
                <Segmented
                  size="sm"
                  value={dim}
                  onChange={setDim}
                  options={[
                    { value: "2d", label: t("review.view2d") },
                    { value: "3d", label: t("review.view3d") },
                  ]}
                />
              )}
            </div>
            {dim === "3d" && can3d ? (
              <Track3D
                review={review}
                ground={ground}
                why={why}
                surface={surface}
                lines={lines}
                allLaps={laps === "all"}
                lap={lap}
                selected={selected}
                className="h-full w-full"
              />
            ) : (
              <div className="h-full p-3">
                <TrackMap review={review} surface={relief} others={others} selected={selected} cursor={cursor} onPick={pick} solo={noTrace} />
              </div>
            )}
            {laps === "all" && dim === "2d" && (
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

          <SetupFixes path={path} findings={review.setup} />
          <LiveCues path={path} lap={lap} reference={compare} />
          <Rivals rivals={data?.rivals ?? []} />

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

/** The other riders in the session worth comparing with, and where each gains on this lap.
 *  Empty, and hidden, for recordings from before the recorder saw other riders. */
function Rivals({ rivals }: { rivals: Rival[] }) {
  const t = useT();
  if (rivals.length === 0) return null;
  return (
    <div>
      <Label>{t("rivals.title")}</Label>
      <div className="space-y-3 border border-border bg-card px-4 py-3">
        {rivals.map((r) => (
          <div key={`${r.num}-${r.lap}`}>
            <div className="eyebrow">{t(`rivals.${r.why}` as TKey)}</div>
            <div className="mt-0.5 flex flex-wrap items-baseline justify-between gap-2">
              <span className="text-[13px] text-foreground">
                {r.name}
                {r.bike && <span className="ml-1.5 text-[12px] text-muted-foreground">{r.bike}</span>}
              </span>
              <span className="font-mono text-[12px] text-muted-foreground">
                {t("rivals.lap", { lap: r.lap, time: lapTime(r.timeMs) })}
              </span>
            </div>
            {r.gains.length === 0 ? (
              <p className="mt-1 text-[12px] text-muted-foreground">{t("rivals.even")}</p>
            ) : (
              <ol className="mt-1.5 space-y-1">
                {r.gains.map((g) => (
                  <li key={g.section} className="grid grid-cols-[1fr_auto] gap-x-2 text-[12.5px]">
                    <span className="text-muted-foreground">{g.section}</span>
                    <span className="font-mono text-accent-foreground">{`−${g.gain.toFixed(2)} s`}</span>
                  </li>
                ))}
              </ol>
            )}
          </div>
        ))}
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
          <div className="eyebrow">
            {t(`review.kind.${s.kind}` as TKey)}
            {s.soil && ` · ${t(`soil.${s.soil.kind}` as TKey)}`}
          </div>
          <div className="mt-0.5 headline text-[24px]">{s.name}</div>
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
