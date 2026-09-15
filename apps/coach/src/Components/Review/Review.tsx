import { useEffect, useMemo, useState } from "react";
import { toast } from "sonner";
import { loadTrackOverview, loadTrackTerrain } from "@frost/shared/api/tracks";
import type { TrackOverview, TrackTerrain } from "@frost/shared/types";
import { ChevronLeft, ChevronRight } from "lucide-react";
import { Button } from "@frost/shared/Components/ui/button";
import { Segmented } from "@frost/shared/Components/ui/segmented";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@frost/shared/Components/ui/select";
import { cn } from "@frost/shared/lib/utils";
import { useT, type TKey } from "@/i18n";
import {
  coachGround,
  coachLines,
  coachReview,
  coachSaveSetup,
  coachSessions,
  coachSetupPlan,
  coachWriteCues,
  type CueAmount,
  type CueLevel,
  type CuesOut,
  coachSurface,
  type Finding,
  type Ground,
  type Lines,
  type ReviewOut,
  type SectionReview,
  type SessionSummary,
  type SetupChange,
  type SetupFix,
  type SetupPlan,
  type Surface,
  type Theme,
} from "@/api/coach";
import { gap, lapTime, lossColor, started } from "@/lib/format";
import Page, { Label } from "../Page";
import TrackMap from "./TrackMap";
import Track3D from "./Track3D";
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
                <TrackMap review={review} surface={relief} others={others} selected={selected} cursor={cursor} onPick={pick} solo={solo} />
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

          <Setup path={path} findings={review.setup} />
          <LiveCues path={path} lap={lap} />

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

const SUSPENSION = ["setup_brake_dive", "setup_exit_squat", "setup_shock_kick", "setup_rear_low", "setup_front_low"];
const SETUP_GROUPS: { key: TKey; of: (skill: string) => boolean }[] = [
  {
    key: "review.group.suspension",
    of: (s) =>
      s.startsWith("setup_bottoming") ||
      s.startsWith("setup_stiff") ||
      s.startsWith("setup_packing") ||
      s.startsWith("setup_sag") ||
      SUSPENSION.includes(s),
  },
  { key: "review.group.gearing", of: (s) => s.startsWith("setup_gearing") },
  { key: "review.group.shifting", of: (s) => s.startsWith("setup_shift") },
  { key: "review.group.chassis", of: (s) => s === "setup_swingarm" || s === "setup_front_push" },
  { key: "review.group.tyres", of: (s) => s === "setup_pressure" },
];

/** Bike setup advice for the whole lap, by what it's about, with the changes behind each tip
 *  against the setup the rider had on, and a copy of that setup with them made. */
type Feel = { key: TKey; skill: string; end?: 0 | 1; bottom?: boolean };

/** What a rider can say about the bike, each with the setup tip it means. `end` marks the ones
 *  the travel used can argue with. */
const FEELS: Feel[] = [
  { key: "feel.frontBottoms", skill: "setup_bottoming_fork", end: 0, bottom: true },
  { key: "feel.rearBottoms", skill: "setup_bottoming_shock", end: 1, bottom: true },
  { key: "feel.frontHarsh", skill: "setup_stiff_fork", end: 0 },
  { key: "feel.rearHarsh", skill: "setup_stiff_shock", end: 1 },
  { key: "feel.dives", skill: "setup_brake_dive" },
  { key: "feel.squats", skill: "setup_exit_squat" },
  { key: "feel.kicks", skill: "setup_shock_kick" },
  { key: "feel.packs", skill: "setup_packing_fork" },
  { key: "feel.pushes", skill: "setup_front_push" },
  { key: "feel.unstable", skill: "setup_unstable" },
  { key: "feel.turnsSlow", skill: "setup_turns_slow" },
  { key: "feel.revsOut", skill: "setup_gearing_tall" },
  { key: "feel.bogs", skill: "setup_gearing_short" },
];
/** Telemetry can't see these at all. */
const ONLY_FELT = ["setup_unstable", "setup_turns_slow"];

/** What the laps say about a feel: they show it too, or the travel used says otherwise (the
 *  share, in percent), or neither. */
function feelCheck(f: Feel, findings: Finding[], used: [number, number] | null): { seen: boolean; against?: number } {
  if (findings.some((x) => x.skill === f.skill)) return { seen: true };
  if (f.end != null && used) {
    const u = used[f.end];
    if (f.bottom ? u < 0.9 : u > 0.95) return { seen: false, against: Math.round(u * 100) };
  }
  return { seen: false };
}

function Setup({ path, findings }: { path: string; findings: Finding[] }) {
  const t = useT();
  const [plan, setPlan] = useState<SetupPlan | null>(null);
  const [saving, setSaving] = useState(false);
  const [felt, setFelt] = useState<string[]>([]);
  // Kept apart from the plan so a refetch never flips a feel's check.
  const [used, setUsed] = useState<[number, number] | null>(null);
  useEffect(() => {
    setPlan(null);
    setFelt([]);
    setUsed(null);
  }, [path]);
  const feels = useMemo(
    () => FEELS.filter((f) => felt.includes(f.skill)).map((f) => ({ f, ...feelCheck(f, findings, used) })),
    [felt, findings, used],
  );
  // A feel the laps argue with adds no fix. Keyed on the names so the same list never refetches.
  const key = [...new Set([...findings.map((f) => f.skill), ...feels.filter((x) => x.against == null).map((x) => x.f.skill)])].join(",");
  const skills = useMemo(() => key.split(",").filter(Boolean), [key]);
  useEffect(() => {
    let live = true;
    coachSetupPlan(path, skills)
      .then((p) => {
        if (!live) return;
        setPlan(p);
        setUsed(p.travelUsed);
      })
      .catch(() => {});
    return () => {
      live = false;
    };
  }, [path, skills]);
  const writes = (plan?.saveAs != null && plan.fixes.some((f) => f.changes.some((c) => c.writes))) ?? false;
  const save = async () => {
    setSaving(true);
    try {
      toast.success(t("setup.saved", { name: await coachSaveSetup(path, skills) }));
    } catch (e) {
      toast.error(String(e));
    } finally {
      setSaving(false);
    }
  };
  return (
    <div>
      <Label>{t("review.setup")}</Label>
      <div className="space-y-4 border border-border bg-card px-4 py-3">
        <div>
          <div className="mb-1.5 eyebrow">{t("feel.title")}</div>
          <p className="mb-2 text-[12.5px] text-muted-foreground">{t("feel.body")}</p>
          <div className="flex flex-wrap gap-1.5">
            {FEELS.map((f) => {
              const on = felt.includes(f.skill);
              return (
                <button
                  key={f.skill}
                  type="button"
                  aria-pressed={on}
                  onClick={() => setFelt((v) => (on ? v.filter((s) => s !== f.skill) : [...v, f.skill]))}
                  className={`border px-2 py-1 text-[12px] ${
                    on ? "border-accent-foreground text-accent-foreground" : "border-border text-muted-foreground hover:text-foreground"
                  }`}
                >
                  {t(f.key)}
                </button>
              );
            })}
          </div>
          {feels.length > 0 && (
            <div className="mt-3 space-y-3">
              {feels.map(({ f, seen, against }) => {
                const fix = !seen && against == null ? plan?.fixes.find((x) => x.skill === f.skill) : undefined;
                const says = seen
                  ? t("feel.agrees")
                  : against != null
                    ? t(f.bottom ? "feel.notBottoming" : "feel.usesAll", { pct: against })
                    : t(ONLY_FELT.includes(f.skill) ? "feel.onlyYou" : "feel.notSeen");
                return (
                  <div key={f.skill}>
                    <p className="text-[12.5px]">
                      <span className="text-foreground">{t(f.key)}.</span> <span className="text-muted-foreground">{says}</span>
                    </p>
                    {fix && <Changes fix={fix} />}
                  </div>
                );
              })}
            </div>
          )}
        </div>
        {SETUP_GROUPS.map((g) => {
          const mine = findings.filter((f) => g.of(f.skill));
          if (mine.length === 0) return null;
          return (
            <div key={g.key}>
              <div className="mb-1.5 eyebrow">{t(g.key)}</div>
              {/* Each tip with the changes behind it right under it. */}
              <div className="space-y-3">
                {mine.map((f) => {
                  const fix = plan?.fixes.find((x) => x.skill === f.skill);
                  return (
                    <div key={f.skill + f.title}>
                      <Notes findings={[f]} />
                      {fix && <Changes fix={fix} />}
                    </div>
                  );
                })}
              </div>
            </div>
          );
        })}
        {plan?.sag && (
          <p className="text-[12px] text-muted-foreground">
            {plan.sag.still
              ? t("setup.sagStill", {
                  front: Math.round(plan.sag.metres[0] * 1000),
                  fp: Math.round(plan.sag.share[0] * 100),
                  rear: Math.round(plan.sag.metres[1] * 1000),
                  rp: Math.round(plan.sag.share[1] * 100),
                })
              : t("setup.sagRiding")}
          </p>
        )}
        {plan && (writes || plan.why) && (
          <div className="border-t border-border pt-3">
            {writes ? (
              <div className="flex flex-wrap items-center gap-3">
                <Button size="sm" onClick={save} disabled={saving}>
                  {t("setup.save", { name: plan.saveAs ?? "" })}
                </Button>
                <span className="text-[12px] text-muted-foreground">{t("setup.saveHint")}</span>
              </div>
            ) : (
              <p className="text-[12px] text-muted-foreground">{plan.why}</p>
            )}
          </div>
        )}
      </div>
    </div>
  );
}

const CUE_LEVEL_KEY = "coach-cue-level";
const CUE_AMOUNT_KEY = "coach-cue-amount";

function remembered<T extends string>(key: string, allowed: readonly T[], fallback: T): T {
  try {
    const v = localStorage.getItem(key);
    return v && (allowed as readonly string[]).includes(v) ? (v as T) : fallback;
  } catch {
    return fallback;
  }
}

function remember(key: string, v: string) {
  try {
    localStorage.setItem(key, v);
  } catch {
    /* no storage: the choice lasts the session */
  }
}

const LEVELS = ["new", "intermediate", "subPro", "pro"] as const;
const AMOUNTS = ["few", "normal", "lots"] as const;

/** Live cues for this track and bike: short calls the recorder shows in practice, picked from
 *  where this lap loses time, for the rider's level and how much coaching they want. */
function LiveCues({ path, lap }: { path: string; lap: number }) {
  const t = useT();
  const [level, setLevel] = useState<CueLevel>(() => remembered(CUE_LEVEL_KEY, LEVELS, "intermediate"));
  const [amount, setAmount] = useState<CueAmount>(() => remembered(CUE_AMOUNT_KEY, AMOUNTS, "normal"));
  const [sent, setSent] = useState<CuesOut | null>(null);
  const [busy, setBusy] = useState(false);
  const send = async () => {
    setBusy(true);
    try {
      const out = await coachWriteCues(path, lap, level, amount);
      setSent(out);
      toast.success(out.cues.length ? t("cues.sent", { n: out.cues.length }) : t("cues.none"));
    } catch (e) {
      toast.error(String(e));
    } finally {
      setBusy(false);
    }
  };
  return (
    <div>
      <Label>{t("cues.title")}</Label>
      <div className="space-y-3 border border-border bg-card px-4 py-3">
        <p className="text-[12.5px] text-muted-foreground">{t("cues.body")}</p>
        <div className="space-y-1.5">
          <div className="eyebrow">{t("cues.level")}</div>
          <Segmented
            size="sm"
            value={level}
            onChange={(v) => {
              setLevel(v);
              setSent(null);
              remember(CUE_LEVEL_KEY, v);
            }}
            options={LEVELS.map((v) => ({ value: v, label: t(`cues.level.${v}` as TKey) }))}
          />
        </div>
        <div className="space-y-1.5">
          <div className="eyebrow">{t("cues.amount")}</div>
          <Segmented
            size="sm"
            value={amount}
            onChange={(v) => {
              setAmount(v);
              setSent(null);
              remember(CUE_AMOUNT_KEY, v);
            }}
            options={AMOUNTS.map((v) => ({ value: v, label: t(`cues.amount.${v}` as TKey) }))}
          />
        </div>
        <div className="flex flex-wrap items-center gap-3 pt-1">
          <Button size="sm" onClick={send} disabled={busy}>
            {t("cues.send")}
          </Button>
          <span className="text-[12px] text-muted-foreground">{t("cues.where")}</span>
        </div>
        {sent && sent.cues.length > 0 && (
          <ol className="space-y-1 border-t border-border pt-2">
            {sent.cues.map((c, i) => (
              <li key={i} className="grid grid-cols-[14px_1fr_auto] gap-x-2 text-[12.5px]">
                <span className="font-mono text-faint">{i + 1}</span>
                <span className="text-muted-foreground">{c.section}</span>
                <span className="font-mono text-accent-foreground">{c.text}</span>
              </li>
            ))}
          </ol>
        )}
      </div>
    </div>
  );
}

/** How far one change goes: the values where the bike's file says, else the direction. */
function amount(c: SetupChange, t: ReturnType<typeof useT>): string {
  if (c.fromValue && c.toValue && c.fromValue !== c.toValue) return `${c.fromValue} → ${c.toValue}`;
  if (c.from != null && c.from === c.to) return t("setup.atLimit");
  const n = Math.abs(c.steps);
  if (c.field === "forkOil") return t(c.steps > 0 ? "setup.moreOil" : "setup.lessOil");
  if (c.field === "frontSprocket" || c.field === "rearSprocket") return `${c.steps > 0 ? "+" : "−"}${n}T`;
  if (c.field === "swingarmLength" || c.field === "rodLength") return t(c.steps > 0 ? "setup.longer" : "setup.shorter");
  if (c.field === "forkHeight") return t(c.steps > 0 ? "setup.frontHigher" : "setup.frontLower");
  if (c.field === "forkOffset") return t(c.steps > 0 ? "setup.moreOffset" : "setup.lessOffset");
  const up = c.steps > 0;
  // Rebound reads as slower or faster, preload as more or less, the rest firmer or softer.
  const [one, many]: [TKey, TKey] =
    c.field === "forkRebound" || c.field === "shockRebound"
      ? up ? ["setup.slowerOne", "setup.slowerMany"] : ["setup.fasterOne", "setup.fasterMany"]
      : c.field === "forkPreload" || c.field === "shockPreload"
        ? up ? ["setup.moreOne", "setup.moreMany"] : ["setup.lessOne", "setup.lessMany"]
        : up ? ["setup.firmerOne", "setup.firmerMany"] : ["setup.softerOne", "setup.softerMany"];
  return n === 1 ? t(one) : t(many, { n });
}

/** The changes behind one tip, in the order to try them. */
function Changes({ fix }: { fix: SetupFix }) {
  const t = useT();
  return (
    <ol className="mt-2 space-y-1.5">
      {fix.changes.map((c, i) => (
        <li key={c.field} className="grid grid-cols-[14px_1fr_auto] items-baseline gap-x-2 text-[12.5px]">
          <span className="font-mono text-faint">{i + 1}</span>
          <span>
            <span className="text-foreground">{t(`setupField.${c.field}` as TKey)}</span>{" "}
            <span className="text-muted-foreground">{c.why}</span>
            {!c.writes && c.from !== c.to && (
              <span className="ml-1.5 text-[11px] text-faint">{t("setup.byHand")}</span>
            )}
          </span>
          <span className="whitespace-nowrap font-mono text-[12px] text-accent-foreground">{amount(c, t)}</span>
        </li>
      ))}
    </ol>
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
