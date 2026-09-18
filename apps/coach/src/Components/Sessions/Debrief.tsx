import { useEffect, useMemo, useState } from "react";
import { Button } from "@frost/shared/Components/ui/button";
import { cn } from "@frost/shared/lib/utils";
import Page, { Label } from "../Page";
import SectionStrip from "../Review/SectionStrip";
import TrackMap from "../Review/TrackMap";
import SectionReplay from "../Review/SectionReplay";
import SetupFixes from "../Review/SetupFixes";
import { preloadBike } from "../Review/BikeRender";
import { Overall, SectionPanel } from "../Review/Review";
import {
  coachGround,
  coachLines,
  coachReview,
  coachSession,
  coachSurface,
  type Ground,
  type Lines,
  type ReviewOut,
  type SessionDetail,
  type Surface,
} from "@/api/coach";
import { lapTime } from "@/lib/format";
import { useT } from "@/i18n";

/**
 * The session, read out one thing at a time.
 *
 * A rider finishes a session and wants to know what to do differently. The lap table asked them
 * to pick a lap and then pick one of five tabs first, which is a lot to know before you are
 * told anything. This walks the same findings in the order a coach would give them: what the
 * session was, what it was about, the sections that cost the most with the lap drawn on the
 * track, then the bike.
 *
 * Read on the session's best whole lap, not its last: the mistakes on your best lap are the
 * ones worth fixing. Everything here is the review's own output — no new analysis, so a session
 * recorded months ago debriefs the same way, with whatever the rules know today.
 */
export default function Debrief({
  path,
  onBack,
  onAllLaps,
  onReview,
}: {
  path: string;
  onBack: () => void;
  onAllLaps: () => void;
  onReview: (file: string, lap: number, solo: boolean, trackId: string) => void;
}) {
  const t = useT();
  const [detail, setDetail] = useState<SessionDetail | null>(null);
  const [data, setData] = useState<ReviewOut | null>(null);
  const [surface, setSurface] = useState<Surface | null>(null);
  // The track's own files, for the turn step: the corner is easier to place on the track it is
  // on than on a flat map of it.
  const [ground, setGround] = useState<Ground | null>(null);
  const [why, setWhy] = useState<string | null>(null);
  const [lines, setLines] = useState<Lines | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [step, setStep] = useState(0);
  /** Which way the corner is being looked at. Kept across steps: a rider who asked to watch
      the bike wants to watch the next corner too. */
  const [view, setView] = useState<"line" | "replay">("line");

  useEffect(() => {
    setDetail(null);
    setData(null);
    setSurface(null);
    setGround(null);
    setWhy(null);
    setLines(null);
    setError(null);
    setStep(0);
    let live = true;
    coachSession(path)
      .then(async (d) => {
        if (!live) return;
        setDetail(d);
        // The best whole lap of the session, wherever in the stints it sits.
        const best = d.summary.laps.filter((l) => l.whole && !l.invalid).sort((a, b) => a.timeMs - b.timeMs)[0];
        if (!best) return;
        const out = await coachReview(best.path, best.num, {});
        if (live) setData(out);
      })
      .catch((e) => live && setError(String(e)));
    // The bike is the slowest thing on any step, so it starts loading with the debrief rather
    // than when the rider gets to it. By then it is already in hand.
    coachSession(path)
      .then((d) => {
        if (live) preloadBike(d.summary.bikeId);
      })
      .catch(() => {});
    coachSurface(path)
      .then((s) => live && setSurface(s))
      .catch(() => {});
    coachGround(path)
      .then((a) => {
        if (!live) return;
        setGround(a.ground);
        setWhy(a.why);
      })
      .catch(() => {});
    coachLines(path)
      .then((l) => live && setLines(l))
      .catch(() => {});
    return () => {
      live = false;
    };
  }, [path]);

  const review = data?.review ?? null;
  // The sections worth walking: the review's own focus list, in the order it ranked them.
  const focus = useMemo(() => review?.focus ?? [], [review]);
  // One step for the verdict, one for the themes, one per focus section, one for the bike.
  const steps = useMemo(() => ["verdict", "themes", ...focus.map((i) => `section:${i}`), "bike"], [focus]);
  const here = steps[Math.min(step, steps.length - 1)] ?? "verdict";
  const last = step >= steps.length - 1;

  if (error) {
    return (
      <Page title={t("debrief.title")} onBack={onBack} backLabel={t("nav.sessions")}>
        <p className="border border-border px-4 py-3 text-[12.5px] text-destructive">{error}</p>
      </Page>
    );
  }
  if (!detail || !review || !data) {
    return (
      <Page title={t("debrief.title")} onBack={onBack} backLabel={t("nav.sessions")}>
        <p className="text-[12.5px] text-muted-foreground">{t("common.loading")}</p>
      </Page>
    );
  }

  const s = detail.summary;
  const ideal = detail.ideal;
  const inYou = ideal && s.bestMs != null ? s.bestMs / 1000 - ideal.time : 0;
  const sel = here.startsWith("section:") ? Number(here.slice(8)) : null;
  const solo = review.solo;

  return (
    <Page
      title={t("debrief.headline", { n: s.laps.filter((l) => l.whole && !l.invalid).length, track: s.trackName || s.trackId })}
      sub={`${s.bikeName || s.bikeId} · ${t("debrief.readOn", { lap: data.lap.lap })}`}
      onBack={onBack}
      backLabel={t("nav.sessions")}
      wide
      fill
      actions={<Button size="sm" variant="outline" onClick={onAllLaps}>{t("debrief.allLaps")}</Button>}
    >
      <div className="flex min-h-0 flex-1 flex-col">
        {/* Only the step scrolls. The rider should never have to scroll to find Next. */}
        <div className="min-h-0 flex-1 overflow-y-auto pr-1">
          {here === "verdict" && (
            <div>
              <div className="grid gap-3 sm:grid-cols-3">
                <Stat label={t("session.best")} value={s.bestMs != null ? lapTime(s.bestMs) : "—"} />
                <Stat
                  label={t("debrief.idealLap")}
                  value={ideal ? lapTime(Math.round(ideal.time * 1000)) : "—"}
                  note={ideal && inYou > 0.01 ? t("debrief.inYou", { s: inYou.toFixed(2) }) : undefined}
                />
                <Stat label={t("debrief.laps")} value={String(s.laps.length)} note={t("debrief.lapsNote", { n: s.laps.filter((l) => l.whole && !l.invalid).length })} />
              </div>
              <div className="mt-5 border border-border bg-card px-5 py-4">
                <p className="text-[15px] leading-relaxed">{t("debrief.verdictBody", { n: focus.length })}</p>
              </div>
            </div>
          )}

          {here === "themes" && (
            <div className="space-y-5">
              <div>
                <Label>{t("debrief.theLap")}</Label>
                <SectionStrip review={review} selected={null} onPick={() => {}} />
              </div>
              <Overall themes={review.overall} solo={solo} onPick={() => setStep(2)} />
            </div>
          )}

          {sel != null && review.sections[sel] && (
            <div className="space-y-4">
              <SectionStrip review={review} selected={sel} onPick={() => {}} />
              <div className="grid gap-4 lg:grid-cols-[1.3fr_1fr]">
                <div className="min-w-0 space-y-2">
                  {/* The line from above, or the bike doing it. Two ways to look at one corner,
                      in one slot, because the step should not grow a scrollbar to hold both. */}
                  <div className="flex gap-1">
                    {(["line", "replay"] as const).map((v) => (
                      <button
                        key={v}
                        onClick={() => setView(v)}
                        className={cn(
                          "border px-2.5 py-1 text-[11.5px]",
                          view === v
                            ? "border-primary text-primary"
                            : "border-border text-muted-foreground hover:text-foreground",
                        )}
                      >
                        {t(v === "line" ? "debrief.viewLine" : "debrief.viewReplay")}
                      </button>
                    ))}
                  </div>
                  {view === "line" ? (
                    <TrackMap review={review} surface={surface} selected={sel} cursor={null} onPick={() => {}} solo={solo} />
                  ) : (
                    <SectionReplay
                      path={data.lap.path}
                      lap={data.lap.lap}
                      sectionId={review.sections[sel].id}
                      bikeId={data.lap.bikeId}
                      rider={s.rider}
                      review={review}
                      ground={ground}
                      why={why}
                      surface={surface}
                      lines={lines}
                      selected={sel}
                    />
                  )}
                </div>
                <div className="space-y-3">
                  <SectionPanel s={review.sections[sel]} solo={solo} onPrev={() => setStep((v) => Math.max(1, v - 1))} onNext={() => setStep((v) => v + 1)} />
                  <Button
                    size="sm"
                    variant="outline"
                    onClick={() => onReview(data.lap.path, data.lap.lap, solo, data.trackId)}
                  >
                    {t("debrief.fullReview", { lap: data.lap.lap })}
                  </Button>
                </div>
              </div>
            </div>
          )}

          {here === "bike" && <SetupFixes path={data.lap.path} findings={review.setup} bikeId={data.lap.bikeId} />}
        </div>

        {/* One thing per screen, so the rider is never asked to choose before being told. */}
        <div className="mt-5 flex shrink-0 items-center gap-4 border-t border-border pt-4">
          <Button size="sm" variant="outline" onClick={() => setStep((v) => Math.max(0, v - 1))} disabled={step === 0}>
            {t("debrief.back")}
          </Button>
          <div className="flex items-center gap-1.5">
            {steps.map((k, i) => (
              <span
                key={k}
                className="h-1.5 rounded-full transition-all"
                style={{
                  width: i === step ? 18 : 6,
                  background: i === step ? "var(--primary)" : i < step ? "var(--faint)" : "var(--secondary)",
                }}
              />
            ))}
          </div>
          <span className="ml-auto text-[12px] text-faint">
            {last ? t("debrief.thatsIt") : t("debrief.next", { what: nextLabel(steps[step + 1] ?? "", review, t) })}
          </span>
          {!last && (
            <Button size="sm" onClick={() => setStep((v) => v + 1)}>
              {t("debrief.nextButton")}
            </Button>
          )}
        </div>
      </div>
    </Page>
  );
}

/** What the next screen is about, so Next is never a step into the dark. */
function nextLabel(key: string, review: ReviewOut["review"], t: ReturnType<typeof useT>): string {
  if (key === "themes") return t("debrief.nextThemes");
  if (key === "bike") return t("debrief.nextBike");
  if (key.startsWith("section:")) {
    const i = Number(key.slice(8));
    return review.sections[i]?.name ?? "";
  }
  return "";
}

function Stat({ label, value, note }: { label: string; value: string; note?: string }) {
  return (
    <div className="border border-border bg-card px-4 py-3">
      <div className="eyebrow">{label}</div>
      <div className="mt-1 font-mono text-[22px] tabular-nums">{value}</div>
      {note && <div className="mt-0.5 text-[12px] text-muted-foreground">{note}</div>}
    </div>
  );
}
