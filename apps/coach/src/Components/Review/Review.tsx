import { useEffect, useMemo, useState } from "react";
import { loadTrackOverview, loadTrackTerrain } from "@frost/shared/api/tracks";
import type { TrackOverview, TrackTerrain } from "@frost/shared/types";
import { ChevronLeft, ChevronRight } from "lucide-react";
import { Button } from "@frost/shared/Components/ui/button";
import { Segmented } from "@frost/shared/Components/ui/segmented";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@frost/shared/Components/ui/tabs";
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
import { ALONE, IDEAL, refArgs, rememberRef, rememberedRef, type Reference } from "@/lib/reference";
import Page, { Label } from "../Page";
import RefPicker, { ReferenceLine } from "./RefPicker";
import TrackMap from "./TrackMap";
import Track3D from "./Track3D";
import SectionStrip from "./SectionStrip";
import Charts from "./Charts";
import SetupFixes, { Notes, Num } from "./SetupFixes";
import BikeSuspension from "./BikeSuspension";
import SectionReplay from "./SectionReplay";
import LiveCues from "./LiveCues";
import HudPanel from "./HudPanel";

/**
 * The page is a lot to take in at once, so it's split.
 *
 * The order is the order a rider works through it: the lap end to end, then corner by corner,
 * then the track those corners are on, then the bike, and last what the game shows while
 * riding — which is a setting rather than a thing to read, so it goes at the end.
 */
const TABS = ["lap", "sections", "setup", "ingame"] as const;
/** How a corner is being looked at: from above, or on the track it is on. */
const VIEWS = ["flat", "solid"] as const;
type View = (typeof VIEWS)[number];
/** Under this a section cost nothing, and `analysis.rs` keeps no tips for it — see `th::WORTH_S`.
 *  The page needs the same number to say why a section has nothing under it. */
const WORTH_S = 0.05;
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
  const [tab, setTab] = useState<Tab>(firstTab);
  const [view, setView] = useState<View>("flat");
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
    coachReview(path, lap, refArgs(compare))
      .then((r) => {
        setData(r);
        setSelected(r.review.focus[0] ?? null);
      })
      .catch((e) => setError(String(e)));
  }, [path, lap, compare]);

  const count = data?.review.sections.length ?? 0;
  // Picking a section anywhere goes to the tab that shows it — except on the track, which
  // already shows it. Clicking a line note there used to throw the rider off the 3D view and
  // onto the sections tab, which is the one place the note is about.
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
  /**
   * How much each tab has for this lap.
   *
   * Not a decoration: five tabs with nothing to tell them apart is the reason a rider opens all
   * five. A clean lap with no setup fixes should say so on the tab rather than after a click.
   * "Lap" counts the themes because that is what the tab is for; "In game" counts nothing,
   * since what the game shows is a setting and not a finding.
   */
  const counts: Record<Tab, number> = {
    lap: review.overall.length,
    sections: worth.length,
    setup: review.setup.length,
    ingame: 0,
  };
  // Every lap's line, fastest green to slowest red; this lap in blue on top.
  const others = (() => {
    if (laps !== "all" || !lines) return undefined;
    const times = lines.laps.map((l) => l.time);
    const [lo, hi] = [Math.min(...times), Math.max(...times)];
    return lines.laps
      // The lines cover every stint of the session, and each stint starts counting at lap 1
      // again: it takes both to leave out the lap that's already drawn in blue.
      .filter((l) => !(l.lap === lap && l.stint === lines.stint))
      .map((l) => ({ path: l.path, colour: `hsl(${Math.round(120 * (1 - (l.time - lo) / Math.max(hi - lo, 0.01)))} 65% 55%)` }));
  })();
  const canAll = lines != null && lines.laps.length > 1;
  const can3d = ground != null || surface != null;
  const bySection = (name: string) => {
    const i = review.sections.findIndex((s) => s.name === name);
    if (i >= 0) pick(i);
  };
  // Held against a lap that isn't faster, every section is a gain, `analysis.rs` keeps only the
  // safety tips and the page goes blank. That happens on exactly the lap a rider is most likely
  // to open — their fastest — so say why, and offer the one comparison a best lap still has
  // something to take from: their own best sections added up.
  const nothing: { title?: TKey; why: TKey; ideal: boolean } | null =
    solo || total > 0
      ? null
      : reference.kind === "ideal"
        ? { why: "review.idealBeaten", ideal: false }
        : data.bestHere
          ? { title: "review.bestLapTitle", why: "review.bestLapWhy", ideal: true }
          : { title: "review.refSlowerTitle", why: "review.refSlowerWhy", ideal: true };
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
                {t("review.against")}{" "}
                <ReferenceLine reference={reference} lapPath={path} lapBikeId={data.lap.bikeId} idealFrom={data.idealFrom} />
              </>
            )}
          </div>
          {noTrace && !solo && <div className="mt-0.5 text-faint">{t("review.idealSub")}</div>}
        </>
      }
      actions={<RefPicker trackId={trackId} path={path} lap={lap} bikeId={data.lap.bikeId} value={compare} onChange={pickRef} />}
      onBack={onBack}
      backLabel={back}
    >
      {nothing && (
        <div className="mb-4 border border-primary/40 bg-card px-4 py-3">
          {nothing.title && <div className="text-[13px] font-semibold">{t(nothing.title)}</div>}
          <div className="mt-0.5 text-[12.5px] leading-snug text-muted-foreground">{t(nothing.why)}</div>
          {nothing.ideal && (
            <Button size="sm" variant="outline" className="mt-2.5" onClick={() => pickRef(IDEAL)}>
              {t("review.useIdeal")}
            </Button>
          )}
        </div>
      )}

      <Tabs value={tab} onValueChange={(v) => setTab(v as Tab)}>
        {/* A tab's name says what it holds; the count says whether it holds anything for this
            lap. Between them a rider can tell what to open without opening all five. */}
        <TabsList>
          {TABS.map((id) => (
            <TabsTrigger key={id} value={id} className="gap-1.5 px-3.5 py-1.5">
              {t(`review.tab.${id}` as TKey)}
              {counts[id] > 0 && (
                <span
                  className={cn(
                    "rounded-full px-1.5 text-[10px] font-semibold tabular-nums",
                    tab === id ? "bg-primary/15 text-primary" : "bg-secondary text-muted-foreground",
                  )}
                >
                  {counts[id]}
                </span>
              )}
            </TabsTrigger>
          ))}
        </TabsList>
        {/* One line on arrival, so "when do I come here" is answered by the page rather than
            by opening it and guessing. */}
        <p className="mb-4 mt-2 text-[12px] text-muted-foreground">{t(`review.tabSub.${tab}` as TKey)}</p>

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
          {lines && lines.notes.length > 0 && (
            <button
              onClick={() => {
                setTab("sections");
                setView("solid");
              }}
              className="flex w-full items-center justify-between gap-3 border border-border bg-card px-4 py-2.5 text-left hover:border-foreground/30"
            >
              <span className="text-[12.5px]">{t("review.linesHere", { count: lines.notes.length })}</span>
              <span className="shrink-0 text-[12px] font-medium text-primary">{t("review.linesOpen")}</span>
            </button>
          )}
        </TabsContent>

        {/* One corner at a time, and the track it is on. These were two tabs, which asked the
            rider to hold a corner in their head while moving between them: the map found it,
            the track showed it, and neither said what the other knew. One tab, two ways of
            looking, and the section stays picked across both. */}
        <TabsContent value="sections" className="space-y-4">
          <SectionStrip review={review} selected={selected} onPick={pick} />
          <div className="grid gap-4 xl:grid-cols-[minmax(0,1fr)_420px]">
            <div className="min-w-0 space-y-2">
              <div className="flex items-center gap-1">
                {VIEWS.map((v) => (
                  <button
                    key={v}
                    onClick={() => setView(v)}
                    disabled={v === "solid" && !can3d}
                    title={v === "solid" && !can3d ? `${t("review.groundFromLaps")}${why ? ` ${why}.` : ""}` : undefined}
                    className={cn(
                      "border px-2.5 py-1 text-[11.5px] disabled:opacity-40",
                      view === v ? "border-primary text-primary" : "border-border text-muted-foreground hover:text-foreground",
                    )}
                  >
                    {t(v === "flat" ? "review.view2d" : "review.view3d")}
                  </button>
                ))}
                {allLaps && <div className="ml-auto">{allLaps}</div>}
              </div>
              {view === "flat" || !can3d ? (
                <div className="relative h-[460px] border border-border bg-card">
                  <div className="h-full p-3">
                    <TrackMap review={review} surface={relief} others={others} selected={selected} cursor={cursor} onPick={pick} solo={noTrace} />
                  </div>
                  {laps === "all" && (
                    <p className="pointer-events-none absolute bottom-2 left-3 text-[11px] text-muted-foreground">{t("review.lapsLegend")}</p>
                  )}
                </div>
              ) : sel ? (
                /* On the track, with the corner played back on the rider's own bike: the 3D
                   view and the replay are the same picture, so they are not two things. */
                <SectionReplay
                  path={path}
                  lap={lap}
                  sectionId={sel.id}
                  bikeId={data.lap.bikeId}
                  rider={data.lap.rider}
                  review={review}
                  ground={ground}
                  why={why}
                  surface={surface}
                  lines={lines}
                  selected={selected}
                  onNext={selected != null ? () => pick((selected + 1) % count) : undefined}
                  onPick={pick}
                />
              ) : (
                <div className="h-[460px] border border-border bg-card">
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
              )}
            </div>
            <div className="space-y-4">
              {sel && selected != null && (
                <SectionPanel
                  key={sel.name}
                  s={sel}
                  solo={solo}
                  onPrev={() => pick((selected - 1 + count) % count)}
                  onNext={() => pick((selected + 1) % count)}
                />
              )}
              {lines && <LineNotes lines={lines} selected={sel?.name ?? null} onPick={bySection} />}
              <Rivals rivals={data.rivals ?? []} />
            </div>
          </div>
        </TabsContent>

        {/* The bike: what it would change, and how it feels. */}
        <TabsContent value="setup">
          <div className="space-y-6">
            {/* The bike and what to change on it first. The readings fold in underneath: they
                are what the changes were worked out from, not the thing to read first. */}
            <SetupFixes path={path} findings={review.setup} bikeId={data.lap.bikeId} />
            <BikeSuspension channels={review.channels} />
          </div>
        </TabsContent>

        {/* What the rider gets while riding. */}
        <TabsContent value="ingame">
          <div className="grid gap-6 lg:grid-cols-2">
            <LiveCues path={path} lap={lap} reference={compare} />
            <HudPanel />
          </div>
        </TabsContent>

      </Tabs>
    </Page>
  );
}

/** The lines the rider took and how the track changed under them — and, where there are none,
 *  why there are none. Every note here needs either several laps through the same corner or
 *  other riders to watch, so a short session ridden alone can't produce one however it was
 *  ridden; an empty panel reads as the coach having nothing to say about the riding. */
export function LineNotes({
  lines,
  selected = null,
  onPick,
}: {
  lines: Lines;
  /** The section on show, so its note stands out. */
  selected?: string | null;
  /** Where a note can be clicked through to its section; a page without sections leaves it out. */
  onPick?: (section: string) => void;
}) {
  const t = useT();
  return (
    <div>
      <Label>{t("review.linesTitle")}</Label>
      {lines.notes.length > 0 ? (
        <div className="space-y-1">
          {lines.notes.map((n, k) => (
            <button
              key={k}
              onClick={onPick ? () => onPick(n.name) : undefined}
              disabled={!onPick}
              className={cn(
                "w-full border bg-card px-4 py-3 text-left",
                selected === n.name ? "border-primary/60" : "border-border",
                onPick && "hover:border-foreground/30",
              )}
            >
              <div className="text-[13px] font-semibold">{n.title}</div>
              <div className="mt-0.5 text-[12.5px] leading-snug text-muted-foreground">{n.detail}</div>
            </button>
          ))}
        </div>
      ) : (
        <div className="border border-border bg-card px-4 py-3 text-[12.5px] text-muted-foreground">
          <p>{t("review.linesNone")}</p>
          <ul className="mt-1.5 list-disc space-y-1 pl-4 text-faint">
            {!lines.enoughLaps && <li>{t("review.linesFewLaps", { count: lines.laps.length })}</li>}
            {lines.alone && <li>{t("review.linesAlone")}</li>}
            {lines.enoughLaps && !lines.alone && <li>{t("review.linesSame")}</li>}
          </ul>
        </div>
      )}
    </div>
  );
}

/** Where the time went first, then anything flagged that cost nothing. */
export function Focus({
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
export function Overall({ themes, solo, onPick }: { themes: Theme[]; solo: boolean; onPick: (section: string) => void }) {
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
export function SectionPanel({ s, solo, onPrev, onNext }: { s: SectionReview; solo: boolean; onPrev: () => void; onNext: () => void }) {
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
          // A section that cost nothing has no tips by design — say that, rather than leaving
          // "Nothing to fix here" to read as a verdict on a section that was never judged.
          <p className="text-[12.5px] text-muted-foreground">
            {t(!solo && s.lost <= WORTH_S ? "review.nothingLost" : "review.nothingHere")}
          </p>
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
