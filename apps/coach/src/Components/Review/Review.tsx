import { useEffect, useMemo, useState } from "react";
import { loadTrackOverview, loadTrackTerrain } from "@frost/shared/api/tracks";
import type { TrackOverview, TrackTerrain } from "@frost/shared/types";
import { ChevronLeft, ChevronRight } from "lucide-react";
import { Button } from "@frost/shared/Components/ui/button";
import { Segmented } from "@frost/shared/Components/ui/segmented";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@frost/shared/Components/ui/select";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@frost/shared/Components/ui/tabs";
import { cn } from "@frost/shared/lib/utils";
import { useT, type TKey } from "@/i18n";
import {
  coachGround,
  coachLines,
  coachReview,
  coachSessions,
  coachSurface,
  type Ground,
  type Lines,
  type ReviewOut,
  type Rival,
  type SectionReview,
  type SessionSummary,
  type Surface,
  type Theme,
} from "@/api/coach";
import { gap, lapTime, lossColor, started } from "@/lib/format";
import Page, { Label } from "../Page";
import TrackMap from "./TrackMap";
import Track3D from "./Track3D";
import SectionStrip from "./SectionStrip";
import Charts from "./Charts";
import SetupFixes, { Notes, Num } from "./SetupFixes";
import LiveCues from "./LiveCues";
import HudPanel from "./HudPanel";

/** The page is a lot to take in at once, so it's split: the lap, the sections, the bike, what
 *  the game shows, and the track itself. */
const TABS = ["lap", "sections", "setup", "ingame", "track"] as const;
type Tab = (typeof TABS)[number];
const TAB_KEY = "coach-review-tab";

function firstTab(): Tab {
  try {
    const v = localStorage.getItem(TAB_KEY);
    return TABS.includes(v as Tab) ? (v as Tab) : "lap";
  } catch {
    return "lap";
  }
}

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
  // What the lap is held against: the fastest on the track, nothing, or a lap picked by hand
  // (`<session path>::<lap>`).
  const [compare, setCompare] = useState<string>(startAlone ? "alone" : "best");
  const [sessions, setSessions] = useState<SessionSummary[]>([]);
  const [data, setData] = useState<ReviewOut | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [selected, setSelected] = useState<number | null>(null);
  const [cursor, setCursor] = useState<number | null>(null);
  const [whole, setWhole] = useState(false);
  const [surface, setSurface] = useState<Surface | null>(null);
  const [tab, setTab] = useState<Tab>(firstTab);
  // Which lines are drawn: this lap alone, or every lap in the session.
  const [laps, setLaps] = useState<"one" | "all">("one");
  const [lines, setLines] = useState<Lines | null>(null);
  const [real, setReal] = useState<{ terrain: TrackTerrain; overview: TrackOverview | null; lift: number } | null>(null);
  const [ground, setGround] = useState<Ground | null>(null);
  const [why, setWhy] = useState<string | null>(null);

  // The tab lasts the session, so flipping between laps doesn't send the rider back to the top.
  useEffect(() => {
    try {
      localStorage.setItem(TAB_KEY, tab);
    } catch {
      /* no storage: the choice lasts this page */
    }
  }, [tab]);

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
    const cut = compare.lastIndexOf("::");
    const [refPath, refLap] = cut > 0 ? [compare.slice(0, cut), Number(compare.slice(cut + 2))] : [undefined, undefined];
    coachReview(path, lap, refPath, refLap, compare === "alone")
      .then((r) => {
        setData(r);
        setSelected(r.review.focus[0] ?? null);
      })
      .catch((e) => setError(String(e)));
  }, [path, lap, compare]);

  // Every session, for the laps on this track the rider can pick to compare with.
  useEffect(() => {
    coachSessions().then(setSessions).catch(() => {});
  }, []);

  const count = data?.review.sections.length ?? 0;
  // Picking a section anywhere goes to the tab that shows it.
  const pick = (i: number) => {
    setSelected(i);
    setWhole(false);
    setTab("sections");
  };
  // The arrow keys walk the sections, but only where a section is on screen.
  useEffect(() => {
    if (tab !== "sections" && tab !== "lap") return;
    const key = (e: KeyboardEvent) => {
      if (!count || (e.key !== "ArrowLeft" && e.key !== "ArrowRight")) return;
      const d = e.key === "ArrowLeft" ? -1 : 1;
      setSelected((s) => ((s ?? (d > 0 ? -1 : 0)) + d + count) % count);
      setWhole(false);
    };
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  }, [count, tab]);

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
  const against = [
    `${t("session.lap")} ${reference.lap + 1}`,
    lapTime(reference.timeMs),
    reference.path === path ? "" : started(reference.started),
  ]
    .filter(Boolean)
    .join(" · ");
  const where = [data.trackName || data.trackId, started(data.lap.started), data.lap.bikeName].filter(Boolean).join(" · ");
  // The fastest lap and nothing, then every other whole lap on this track, this session first.
  const choices = [
    { value: "best", label: t("review.fastest") },
    { value: "alone", label: t("review.alone") },
    ...[...sessions]
      .filter((s) => s.trackId === data.trackId)
      .sort((a, b) => (a.path === path ? -1 : b.path === path ? 1 : 0))
      .flatMap((s) =>
        s.laps
          .filter((l) => l.whole && !l.invalid && !(s.path === path && l.num === lap))
          .map((l) => ({
            value: `${s.path}::${l.num}`,
            label: [`${t("session.lap")} ${l.num + 1}`, lapTime(l.timeMs), s.path === path ? "" : started(s.started)]
              .filter(Boolean)
              .join(" · "),
          })),
      ),
  ];
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
  const allLaps = canAll && (
    <Segmented
      size="sm"
      value={laps}
      onChange={setLaps}
      options={[
        { value: "one", label: t("review.thisLap") },
        { value: "all", label: t("review.viewLaps") },
      ]}
    />
  );

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
                {t("review.against")} {against}
              </>
            )}
          </div>
        </>
      }
      actions={
        <div className="flex items-center gap-2">
          <span className="text-[11.5px] text-muted-foreground">{t("review.compareWith")}</span>
          <Select value={compare} onValueChange={setCompare}>
            <SelectTrigger className="h-8 w-[260px] text-[12px]">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {choices.map((c) => (
                <SelectItem key={c.value} value={c.value}>
                  {c.label}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>
      }
      onBack={onBack}
      backLabel={back}
    >
      <Tabs value={tab} onValueChange={(v) => setTab(v as Tab)}>
        <TabsList className="mb-4">
          {TABS.map((id) => (
            <TabsTrigger key={id} value={id} className="px-3.5 py-1.5">
              {t(`review.tab.${id}` as TKey)}
            </TabsTrigger>
          ))}
        </TabsList>

        {/* The lap end to end: where the time went, and the charts behind it. */}
        <TabsContent value="lap" className="space-y-4">
          <div>
            <SectionStrip review={review} selected={selected} onPick={pick} />
            <p className="mt-2 text-[11.5px] text-faint">{t("review.strip")}</p>
          </div>
          <div className="border border-border bg-card">
            <Charts review={review} selected={selected} whole={whole} onWhole={setWhole} cursor={cursor} onCursor={setCursor} />
          </div>
          <div className="grid gap-6 lg:grid-cols-2">
            <Overall themes={review.overall} solo={solo} onPick={bySection} />
            <Focus review={review} worth={worth} selected={selected} solo={solo} onPick={pick} />
          </div>
        </TabsContent>

        {/* One section at a time, with the map to find it on. */}
        <TabsContent value="sections" className="space-y-4">
          <SectionStrip review={review} selected={selected} onPick={pick} />
          <div className="grid gap-4 xl:grid-cols-[minmax(0,1fr)_420px]">
            <div className="relative h-[460px] border border-border bg-card">
              {allLaps && <div className="absolute right-3 top-3 z-10">{allLaps}</div>}
              <div className="h-full p-3">
                <TrackMap review={review} surface={relief} others={others} selected={selected} cursor={cursor} onPick={pick} solo={solo} />
              </div>
              {laps === "all" && (
                <p className="pointer-events-none absolute bottom-2 left-3 text-[11px] text-muted-foreground">{t("review.lapsLegend")}</p>
              )}
            </div>
            {sel && selected != null && (
              <SectionPanel
                key={sel.name}
                s={sel}
                solo={solo}
                onPrev={() => pick((selected - 1 + count) % count)}
                onNext={() => pick((selected + 1) % count)}
              />
            )}
          </div>
        </TabsContent>

        {/* The bike: what it would change, and how it feels. */}
        <TabsContent value="setup">
          <SetupFixes path={path} findings={review.setup} />
        </TabsContent>

        {/* What the rider gets while riding. */}
        <TabsContent value="ingame">
          <div className="grid gap-6 lg:grid-cols-2">
            <LiveCues path={path} lap={lap} />
            <HudPanel />
          </div>
        </TabsContent>

        {/* The track itself, with the lap on it. */}
        <TabsContent value="track" className="space-y-4">
          {can3d ? (
            <div className="relative h-[520px] border border-border bg-card">
              {allLaps && <div className="absolute right-3 top-3 z-10">{allLaps}</div>}
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
            </div>
          ) : (
            <p className="border border-border bg-card px-4 py-3 text-[12.5px] text-muted-foreground">
              {`${t("review.groundFromLaps")}${why ? ` ${why}.` : ""}`}
            </p>
          )}
          <div className="grid gap-6 lg:grid-cols-2">
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
            <Rivals rivals={data.rivals ?? []} />
          </div>
        </TabsContent>
      </Tabs>
    </Page>
  );
}

/** Where the time went first, then anything flagged that cost nothing. */
function Focus({
  review,
  worth,
  selected,
  solo,
  onPick,
}: {
  review: ReviewOut["review"];
  worth: number[];
  selected: number | null;
  solo: boolean;
  onPick: (i: number) => void;
}) {
  const t = useT();
  return (
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
                onClick={() => onPick(i)}
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
